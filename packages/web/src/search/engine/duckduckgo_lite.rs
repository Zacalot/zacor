use super::SearchEngine;
use crate::search::SEARCH_USER_AGENT;
use crate::search::engine::duckduckgo_html::{
    decode_duckduckgo_url, detect_response_issue, is_ad_result, normalize_text,
};
use crate::types::{SearchMode, SearchRequest, SearchResult};
use scraper::{Html, Selector};
use zacor_package::io::http;

pub struct DuckDuckGoLiteEngine;

impl SearchEngine for DuckDuckGoLiteEngine {
    fn name(&self) -> &'static str {
        "duckduckgo-lite"
    }

    fn search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>, String> {
        if request.mode != SearchMode::Web {
            return Err("news mode is not supported by this engine".to_string());
        }

        let offset = request.count.saturating_mul(request.page.saturating_sub(1));
        let offset_str = offset.to_string();
        let response = http::fetch(
            &http::Request::get("https://lite.duckduckgo.com/lite/")
                .query(&[("q", request.query.as_str()), ("s", offset_str.as_str())])
                .user_agent(SEARCH_USER_AGENT)
                .timeout_secs(request.timeout_secs),
        )
        .map_err(|e| format!("request failed: {e}"))?;

        if !response.is_success() {
            let status = response.status;
            return Err(match status {
                202 | 403 | 429 => {
                    format!("provider appears rate limited or blocked the request (HTTP {status})")
                }
                _ => format!("provider returned HTTP {status}"),
            });
        }

        let body = response
            .text()
            .map_err(|e| format!("read body failed: {e}"))?;
        if let Some(issue) = detect_response_issue(&body) {
            return Err(issue);
        }
        parse_lite_results(&body, self.name(), request.count, offset)
    }
}

pub(crate) fn parse_lite_results(
    html: &str,
    engine: &str,
    count: usize,
    offset: usize,
) -> Result<Vec<SearchResult>, String> {
    let doc = Html::parse_document(html);
    let row_selector = Selector::parse("tr").map_err(|e| format!("selector: {e}"))?;
    let link_selector = Selector::parse("a.result-link").map_err(|e| format!("selector: {e}"))?;
    let snippet_selector =
        Selector::parse("td.result-snippet").map_err(|e| format!("selector: {e}"))?;

    let rows: Vec<_> = doc.select(&row_selector).collect();
    let mut results = Vec::new();
    let mut index = 0usize;

    while index < rows.len() {
        let row = rows[index];
        let Some(link) = row.select(&link_selector).next() else {
            index += 1;
            continue;
        };

        let raw_url = link.value().attr("href").unwrap_or("");
        if is_ad_result(raw_url) {
            index += 1;
            continue;
        }
        let url = decode_duckduckgo_url(raw_url)?;
        let title = normalize_text(&link.text().collect::<Vec<_>>().join(" "));
        let snippet = rows
            .get(index + 1)
            .and_then(|next| next.select(&snippet_selector).next())
            .map(|node| normalize_text(&node.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();

        if !title.is_empty() && !url.is_empty() {
            results.push(SearchResult {
                engine: engine.to_string(),
                rank: (offset + results.len() + 1) as i64,
                title,
                url,
                snippet,
            });
        }

        if results.len() >= count {
            break;
        }
        index += 1;
    }

    if results.is_empty() {
        if let Some(issue) = detect_response_issue(html) {
            return Err(issue);
        }
        return Err("parse produced no results".to_string());
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::parse_lite_results;

    #[test]
    fn parses_lite_results() {
        let html = r#"
        <table>
          <tr><td>1.</td><td><a class="result-link" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Falpha">Alpha</a></td></tr>
          <tr><td></td><td class="result-snippet">Alpha snippet</td></tr>
          <tr><td>2.</td><td><a class="result-link" href="https://example.com/beta">Beta</a></td></tr>
          <tr><td></td><td class="result-snippet">Beta snippet</td></tr>
        </table>
        "#;

        let results = parse_lite_results(html, "duckduckgo-lite", 10, 0).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].engine, "duckduckgo-lite");
        assert_eq!(results[0].url, "https://example.com/alpha");
        assert_eq!(results[1].rank, 2);
    }

    #[test]
    fn reports_parse_failure_when_no_results_exist() {
        let err = parse_lite_results("<table></table>", "duckduckgo-lite", 10, 0).unwrap_err();
        assert!(err.contains("no results"));
    }

    #[test]
    fn filters_sponsored_results() {
        let html = r#"
        <table>
          <tr><td>1.</td><td><a class="result-link" href="https://duckduckgo.com/y.js?ad_domain=chewy.com&ad_provider=bing&ad_type=txad">Ad Result</a></td></tr>
          <tr><td></td><td class="result-snippet">Sponsored</td></tr>
          <tr><td>2.</td><td><a class="result-link" href="https://example.com/beta">Beta</a></td></tr>
          <tr><td></td><td class="result-snippet">Beta snippet</td></tr>
        </table>
        "#;

        let results = parse_lite_results(html, "duckduckgo-lite", 10, 0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Beta");
        assert_eq!(results[0].rank, 1);
    }
}
