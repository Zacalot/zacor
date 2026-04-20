use super::SearchEngine;
use crate::search::SEARCH_USER_AGENT;
use crate::types::{SearchMode, SearchRequest, SearchResult};
use scraper::{Html, Selector};
use url::Url;
use zacor_package::io::http;

pub struct DuckDuckGoHtmlEngine;

impl SearchEngine for DuckDuckGoHtmlEngine {
    fn name(&self) -> &'static str {
        "duckduckgo"
    }

    fn search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>, String> {
        if request.mode != SearchMode::Web {
            return Err("news mode is not supported by this engine".to_string());
        }

        let offset = request.count.saturating_mul(request.page.saturating_sub(1));
        let offset_str = offset.to_string();
        let response = http::fetch(
            &http::Request::get("https://html.duckduckgo.com/html/")
                .query(&[("q", request.query.as_str()), ("s", offset_str.as_str())])
                .user_agent(SEARCH_USER_AGENT)
                .timeout_secs(request.timeout_secs),
        )
        .map_err(|e| format!("request failed: {e}"))?;

        if !response.is_success() {
            return Err(classify_status(response.status));
        }

        let body = response
            .text()
            .map_err(|e| format!("read body failed: {e}"))?;
        if let Some(issue) = detect_response_issue(&body) {
            return Err(issue);
        }
        parse_html_results(&body, self.name(), request.count, offset)
    }
}

fn classify_status(status: u16) -> String {
    match status {
        202 | 403 | 429 => {
            format!("provider appears rate limited or blocked the request (HTTP {status})")
        }
        _ => format!("provider returned HTTP {status}"),
    }
}

pub(crate) fn parse_html_results(
    html: &str,
    engine: &str,
    count: usize,
    offset: usize,
) -> Result<Vec<SearchResult>, String> {
    let doc = Html::parse_document(html);
    let result_selector = Selector::parse("div.result").map_err(|e| format!("selector: {e}"))?;
    let title_selector = Selector::parse("a.result__a").map_err(|e| format!("selector: {e}"))?;
    let snippet_selector = Selector::parse("a.result__snippet").map_err(|e| format!("selector: {e}"))?;

    let mut results = Vec::new();
    for result in doc.select(&result_selector) {
        let Some(title_link) = result.select(&title_selector).next() else {
            continue;
        };

        let raw_url = title_link.value().attr("href").unwrap_or("");
        if is_ad_result(raw_url) {
            continue;
        }
        let url = decode_duckduckgo_url(raw_url)?;
        let title = normalize_text(&title_link.text().collect::<Vec<_>>().join(" "));
        let snippet = result
            .select(&snippet_selector)
            .next()
            .map(|node| normalize_text(&node.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();

        if title.is_empty() || url.is_empty() {
            continue;
        }

        results.push(SearchResult {
            engine: engine.to_string(),
            rank: (offset + results.len() + 1) as i64,
            title,
            url,
            snippet,
        });

        if results.len() >= count {
            break;
        }
    }

    if results.is_empty() {
        if let Some(issue) = detect_response_issue(html) {
            return Err(issue);
        }
        return Err("parse produced no results".to_string());
    }

    Ok(results)
}

pub(crate) fn decode_duckduckgo_url(raw_url: &str) -> Result<String, String> {
    if raw_url.is_empty() {
        return Ok(String::new());
    }

    let normalized = if raw_url.starts_with("//") {
        format!("https:{raw_url}")
    } else {
        raw_url.to_string()
    };

    if let Ok(parsed) = Url::parse(&normalized)
        && let Some(target) = parsed
            .query_pairs()
            .find_map(|(key, value)| (key == "uddg").then(|| value.to_string()))
    {
        return Ok(target);
    }

    Ok(normalized)
}

pub(crate) fn is_ad_result(raw_url: &str) -> bool {
    if raw_url.is_empty() {
        return false;
    }

    let normalized = if raw_url.starts_with("//") {
        format!("https:{raw_url}")
    } else {
        raw_url.to_string()
    };

    let Ok(parsed) = Url::parse(&normalized) else {
        return false;
    };

    if parsed.host_str() != Some("duckduckgo.com") || parsed.path() != "/y.js" {
        return false;
    }

    parsed.query_pairs().any(|(key, _)| {
        matches!(key.as_ref(), "ad_domain" | "ad_provider" | "ad_type")
    })
}

pub(crate) fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string()
}

pub(crate) fn detect_response_issue(body: &str) -> Option<String> {
    let normalized = body.to_ascii_lowercase();
    let markers = [
        "rate limit",
        "too many requests",
        "unusual traffic",
        "automated requests",
        "request looks automated",
        "verify you are human",
        "verify you're human",
        "captcha",
        "challenge",
        "temporarily unavailable",
    ];

    markers
        .iter()
        .any(|marker| normalized.contains(marker))
        .then(|| "provider appears rate limited or blocked the request".to_string())
}

#[cfg(test)]
mod tests {
    use super::{decode_duckduckgo_url, detect_response_issue, is_ad_result, parse_html_results};

    #[test]
    fn parses_html_results() {
        let html = r#"
        <div class="result">
          <h2 class="result__title"><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fone">Result One</a></h2>
          <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fone">First snippet</a>
        </div>
        <div class="result">
          <h2 class="result__title"><a class="result__a" href="https://example.com/two">Result Two</a></h2>
          <a class="result__snippet" href="https://example.com/two">Second snippet</a>
        </div>
        "#;

        let results = parse_html_results(html, "duckduckgo", 10, 0).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].rank, 1);
        assert_eq!(results[0].url, "https://example.com/one");
        assert_eq!(results[1].rank, 2);
    }

    #[test]
    fn reports_parse_failure_when_no_results_exist() {
        let err = parse_html_results("<html></html>", "duckduckgo", 10, 0).unwrap_err();
        assert!(err.contains("no results"));
    }

    #[test]
    fn decodes_duckduckgo_redirect_url() {
        let decoded = decode_duckduckgo_url(
            "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage",
        )
        .unwrap();
        assert_eq!(decoded, "https://example.com/page");
    }

    #[test]
    fn filters_sponsored_results() {
        let html = r#"
        <div class="result">
          <h2 class="result__title"><a class="result__a" href="https://duckduckgo.com/y.js?ad_domain=chewy.com&ad_provider=bing&ad_type=txad">Ad Result</a></h2>
          <a class="result__snippet" href="https://duckduckgo.com/y.js?ad_domain=chewy.com">Sponsored</a>
        </div>
        <div class="result">
          <h2 class="result__title"><a class="result__a" href="https://example.com/two">Organic Result</a></h2>
          <a class="result__snippet" href="https://example.com/two">Organic snippet</a>
        </div>
        "#;

        let results = parse_html_results(html, "duckduckgo", 10, 0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Organic Result");
        assert_eq!(results[0].rank, 1);
    }

    #[test]
    fn detects_duckduckgo_ad_urls() {
        assert!(is_ad_result(
            "https://duckduckgo.com/y.js?ad_domain=chewy.com&ad_provider=bing&ad_type=txad"
        ));
        assert!(!is_ad_result(
            "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage"
        ));
        assert!(!is_ad_result("https://example.com/page"));
    }

    #[test]
    fn detects_block_page_markers() {
        let body = "We detected unusual traffic. Please verify you are human to continue.";
        assert_eq!(
            detect_response_issue(body).as_deref(),
            Some("provider appears rate limited or blocked the request")
        );
    }
}
