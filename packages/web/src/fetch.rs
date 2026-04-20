use crate::args;
use crate::types::{FetchRecord, FetchRequest};
use serde_json::Value;
use zacor_package::io::http;

const FETCH_CONTEXT: &str = "web fetch";
const DEFAULT_USER_AGENT: &str = "zr-web/0.1";

pub fn run(args: args::FetchArgs) -> Result<FetchRecord, String> {
    let request = request_from_args(&args)?;
    fetch(&request)
}

pub fn request_from_args(args: &args::FetchArgs) -> Result<FetchRequest, String> {
    let url = args.url.trim().to_string();
    if url.is_empty() {
        return Err(format!("{FETCH_CONTEXT}: url is required"));
    }

    let timeout_secs = parse_positive_i64(args.timeout, "timeout")? as u64;
    let user_agent = if args.user_agent.trim().is_empty() {
        DEFAULT_USER_AGENT.to_string()
    } else {
        args.user_agent.trim().to_string()
    };

    Ok(FetchRequest {
        url,
        timeout_secs,
        user_agent,
    })
}

pub fn fetch(request: &FetchRequest) -> Result<FetchRecord, String> {
    let response = http::fetch(
        &http::Request::get(&request.url)
            .user_agent(&request.user_agent)
            .timeout_secs(request.timeout_secs),
    )
    .map_err(|e| format!("{FETCH_CONTEXT}: {e}"))?;

    let content_type = response.content_type().unwrap_or("").to_string();
    let body = response
        .text()
        .map_err(|e| format!("{FETCH_CONTEXT}: body: {e}"))?;

    Ok(FetchRecord {
        url: response.final_url,
        status: response.status,
        elapsed: response.elapsed_ms as f64 / 1000.0,
        body,
        content_type,
    })
}

fn parse_positive_i64(value: i64, field: &str) -> Result<i64, String> {
    if value < 1 {
        return Err(format!("{FETCH_CONTEXT}: {field} must be at least 1"));
    }
    Ok(value)
}

pub fn emit_record(record: FetchRecord) -> Result<Vec<Value>, String> {
    serde_json::to_value(record)
        .map(|value| vec![value])
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(url: &str) -> args::FetchArgs {
        args::FetchArgs {
            url: url.to_string(),
            timeout: 30,
            user_agent: DEFAULT_USER_AGENT.to_string(),
        }
    }

    #[test]
    fn rejects_missing_url() {
        let err = request_from_args(&args("   ")).unwrap_err();
        assert!(err.contains("url is required"));
    }

    #[test]
    fn preserves_fetch_request_shape() {
        let request = request_from_args(&args("https://example.com")).unwrap();
        assert_eq!(request.url, "https://example.com");
        assert_eq!(request.timeout_secs, 30);
        assert_eq!(request.user_agent, DEFAULT_USER_AGENT);
    }

    #[test]
    fn serializes_record_output() {
        let output = emit_record(FetchRecord {
            url: "https://example.com".to_string(),
            status: 200,
            elapsed: 0.25,
            body: "hello".to_string(),
            content_type: "text/plain".to_string(),
        })
        .unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["status"], 200);
        assert_eq!(output[0]["content_type"], "text/plain");
    }
}
