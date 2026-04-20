mod engine;

use crate::args;
use crate::types::{SearchMode, SearchRequest, SearchResult};

pub use engine::{DuckDuckGoHtmlEngine, DuckDuckGoLiteEngine, EngineRegistry, SearchEngine};
use engine::{default_registry, execute_with_fallbacks};

pub const DEFAULT_ENGINE: &str = "duckduckgo";
const SEARCH_CONTEXT: &str = "web search";
pub(crate) const SEARCH_USER_AGENT: &str = "zr-web/0.1";

pub fn run(args: args::SearchArgs) -> Result<Vec<SearchResult>, String> {
    let registry = default_registry();
    let request = request_from_args(&args, &registry)?;
    search_with_registry(&registry, &request)
}

pub fn request_from_args(
    args: &args::SearchArgs,
    registry: &EngineRegistry,
) -> Result<SearchRequest, String> {
    let query = args.query.trim().to_string();
    if query.is_empty() {
        return Err(format!("{SEARCH_CONTEXT}: query is required"));
    }

    let engine = normalize_engine_name(&args.engine);
    if !registry.contains(&engine) {
        return Err(format!(
            "{SEARCH_CONTEXT}: unknown engine '{}'. Valid: {}",
            engine,
            registry.known_engines().join(", ")
        ));
    }

    let count = parse_positive_i64(args.count, "count")? as usize;
    let page = parse_positive_i64(args.page, "page")? as usize;
    let timeout_secs = parse_positive_i64(args.timeout, "timeout")? as u64;

    let fallbacks = if let Some(raw) = args.fallback.as_ref() {
        parse_fallbacks(raw, &engine, registry)?
    } else {
        registry.default_fallbacks_for(&engine)
    };

    Ok(SearchRequest {
        query,
        engine,
        fallbacks,
        count,
        page,
        timeout_secs,
        mode: if args.news {
            SearchMode::News
        } else {
            SearchMode::Web
        },
    })
}

pub fn parse_fallbacks(
    raw: &str,
    primary: &str,
    registry: &EngineRegistry,
) -> Result<Vec<String>, String> {
    let mut fallbacks = Vec::new();
    for item in raw.split(',') {
        let name = normalize_engine_name(item);
        if name.is_empty() || name == primary || fallbacks.iter().any(|existing| existing == &name) {
            continue;
        }
        if !registry.contains(&name) {
            return Err(format!(
                "{SEARCH_CONTEXT}: unknown fallback engine '{}'. Valid: {}",
                name,
                registry.known_engines().join(", ")
            ));
        }
        fallbacks.push(name);
    }
    Ok(fallbacks)
}

pub fn search(request: &SearchRequest) -> Result<Vec<SearchResult>, String> {
    let registry = default_registry();
    search_with_registry(&registry, request)
}

pub fn search_with_registry(
    registry: &EngineRegistry,
    request: &SearchRequest,
) -> Result<Vec<SearchResult>, String> {
    execute_with_fallbacks(registry, request)
}

fn normalize_engine_name(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn parse_positive_i64(value: i64, field: &str) -> Result<i64, String> {
    if value < 1 {
        return Err(format!("{SEARCH_CONTEXT}: {field} must be at least 1"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubEngine {
        name: &'static str,
        result: std::result::Result<Vec<SearchResult>, &'static str>,
    }

    impl SearchEngine for StubEngine {
        fn name(&self) -> &'static str {
            self.name
        }

        fn search(
            &self,
            _request: &SearchRequest,
        ) -> std::result::Result<Vec<SearchResult>, String> {
            self.result.clone().map_err(|err| err.to_string())
        }
    }

    fn request(primary: &str) -> SearchRequest {
        SearchRequest {
            query: "rust async mutex".to_string(),
            engine: primary.to_string(),
            fallbacks: vec!["duckduckgo-lite".to_string()],
            count: 10,
            page: 1,
            timeout_secs: 30,
            mode: SearchMode::Web,
        }
    }

    fn registry() -> EngineRegistry {
        EngineRegistry::new()
            .with_engine(StubEngine {
                name: "duckduckgo",
                result: Err("timeout"),
            })
            .with_engine(StubEngine {
                name: "duckduckgo-lite",
                result: Ok(vec![SearchResult {
                    engine: "duckduckgo-lite".to_string(),
                    rank: 1,
                    title: "Result".to_string(),
                    url: "https://example.com".to_string(),
                    snippet: "Snippet".to_string(),
                }]),
            })
    }

    #[test]
    fn rejects_unknown_engine() {
        let registry = default_registry();
        let args = args::SearchArgs {
            query: "rust".to_string(),
            engine: "bogus".to_string(),
            fallback: None,
            count: 10,
            page: 1,
            timeout: 30,
            news: false,
        };

        let err = request_from_args(&args, &registry).unwrap_err();
        assert!(err.contains("unknown engine"));
    }

    #[test]
    fn rejects_empty_query() {
        let registry = default_registry();
        let args = args::SearchArgs {
            query: "   ".to_string(),
            engine: DEFAULT_ENGINE.to_string(),
            fallback: None,
            count: 10,
            page: 1,
            timeout: 30,
            news: false,
        };

        let err = request_from_args(&args, &registry).unwrap_err();
        assert!(err.contains("query is required"));
    }

    #[test]
    fn parses_known_fallbacks() {
        let registry = default_registry();
        let fallbacks = parse_fallbacks("duckduckgo-lite,duckduckgo", DEFAULT_ENGINE, &registry).unwrap();
        assert_eq!(fallbacks, vec!["duckduckgo-lite".to_string()]);
    }

    #[test]
    fn fallback_succeeds_after_primary_failure() {
        let result = search_with_registry(&registry(), &request("duckduckgo")).unwrap();
        assert_eq!(result[0].engine, "duckduckgo-lite");
    }

    #[test]
    fn all_engines_failed_is_reported() {
        let registry = EngineRegistry::new()
            .with_engine(StubEngine {
                name: "duckduckgo",
                result: Err("timeout"),
            })
            .with_engine(StubEngine {
                name: "duckduckgo-lite",
                result: Err("parse failure"),
            });
        let err = search_with_registry(&registry, &request("duckduckgo")).unwrap_err();
        assert!(err.contains("duckduckgo: timeout"));
        assert!(err.contains("duckduckgo-lite: parse failure"));
    }

    #[test]
    fn explicit_engine_selection_is_preserved() {
        let registry = default_registry();
        let args = args::SearchArgs {
            query: "rust".to_string(),
            engine: "duckduckgo-lite".to_string(),
            fallback: None,
            count: 5,
            page: 1,
            timeout: 15,
            news: false,
        };

        let request = request_from_args(&args, &registry).unwrap();
        assert_eq!(request.engine, "duckduckgo-lite");
    }
}
