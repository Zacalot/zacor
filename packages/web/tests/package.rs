struct StubEngine {
    name: &'static str,
    result: std::result::Result<Vec<zr_web::ResultRow>, &'static str>,
}

impl zr_web::SearchEngine for StubEngine {
    fn name(&self) -> &'static str {
        self.name
    }

    fn search(
        &self,
        _request: &zr_web::Request,
    ) -> std::result::Result<Vec<zr_web::ResultRow>, String> {
        self.result.clone().map_err(|err| err.to_string())
    }
}

fn registry() -> zr_web::EngineRegistry {
    zr_web::EngineRegistry::new()
        .with_engine(StubEngine {
            name: zr_web::DEFAULT_ENGINE,
            result: Err("timeout"),
        })
        .with_engine(StubEngine {
            name: "duckduckgo-lite",
            result: Ok(vec![zr_web::ResultRow {
                engine: "duckduckgo-lite".to_string(),
                rank: 1,
                title: "Result".to_string(),
                url: "https://example.com".to_string(),
                snippet: "Snippet".to_string(),
            }]),
        })
}

fn search_args(query: &str) -> zr_web::args::SearchArgs {
    zr_web::args::SearchArgs {
        query: query.to_string(),
        engine: zr_web::DEFAULT_ENGINE.to_string(),
        fallback: Some("duckduckgo-lite".to_string()),
        count: 10,
        page: 1,
        timeout: 30,
        news: false,
    }
}

fn fetch_args(url: &str) -> zr_web::args::FetchArgs {
    zr_web::args::FetchArgs {
        url: url.to_string(),
        timeout: 30,
        user_agent: "zr-web/0.1".to_string(),
    }
}

#[test]
fn package_api_reports_missing_query() {
    let err = zr_web::search_request_from_args(&search_args("   "), &registry()).unwrap_err();
    assert!(err.contains("query is required"));
}

#[test]
fn package_api_respects_explicit_engine_selection() {
    let mut args = search_args("rust async mutex");
    args.engine = "duckduckgo-lite".to_string();

    let request = zr_web::search_request_from_args(&args, &registry()).unwrap();
    assert_eq!(request.engine, "duckduckgo-lite");
}

#[test]
fn package_api_uses_fallback_on_failure() {
    let request =
        zr_web::search_request_from_args(&search_args("rust async mutex"), &registry()).unwrap();
    let results = zr_web::search_with_registry(&registry(), &request).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].engine, "duckduckgo-lite");
}

#[test]
fn package_api_reports_total_failure() {
    let registry = zr_web::EngineRegistry::new()
        .with_engine(StubEngine {
            name: zr_web::DEFAULT_ENGINE,
            result: Err("timeout"),
        })
        .with_engine(StubEngine {
            name: "duckduckgo-lite",
            result: Err("parse failure"),
        });
    let request =
        zr_web::search_request_from_args(&search_args("rust async mutex"), &registry).unwrap();
    let err = zr_web::search_with_registry(&registry, &request).unwrap_err();
    assert!(err.contains("timeout"));
    assert!(err.contains("parse failure"));
}

#[test]
fn package_api_reports_missing_url() {
    let err = zr_web::fetch::request_from_args(&fetch_args("   ")).unwrap_err();
    assert!(err.contains("url is required"));
}

#[test]
fn package_api_preserves_fetch_request_shape() {
    let request = zr_web::fetch::request_from_args(&fetch_args("https://example.com")).unwrap();
    assert_eq!(request.url, "https://example.com");
    assert_eq!(request.timeout_secs, 30);
}

#[test]
fn package_api_serializes_fetch_output_shape() {
    let record = zr_web::FetchRecord {
        url: "https://example.com".to_string(),
        status: 200,
        elapsed: 0.1,
        body: "ok".to_string(),
        content_type: "text/plain".to_string(),
    };
    let value = serde_json::to_value(record).unwrap();
    assert_eq!(value["url"], "https://example.com");
    assert_eq!(value["status"], 200);
    assert_eq!(value["body"], "ok");
    assert_eq!(value["content_type"], "text/plain");
}
