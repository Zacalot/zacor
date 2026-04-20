//! HTTP IO abstraction.
//!
//! Unlike `io::fs`, http has no local fast path — it always routes through
//! the host's `http.fetch` capability. The host (fat `zr`, daemon, or
//! protocol-serving peer) owns the HTTP client, TLS stack, and redirect
//! policy so packages can stay tiny and remain wasm-compatible.

use crate::protocol;
use serde_json::json;
use std::collections::BTreeMap;
use std::io;

#[derive(Debug, Clone)]
pub struct Request {
    pub url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub timeout_ms: u64,
}

impl Request {
    /// Build a GET request with default 30s timeout and no headers.
    pub fn get(url: impl Into<String>) -> Self {
        Request {
            url: url.into(),
            method: "GET".into(),
            headers: BTreeMap::new(),
            body: None,
            timeout_ms: 30_000,
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn user_agent(self, ua: impl Into<String>) -> Self {
        self.header("user-agent", ua)
    }

    pub fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    pub fn timeout_secs(self, secs: u64) -> Self {
        self.timeout_ms(secs.saturating_mul(1000))
    }

    /// Append query parameters to the URL. Uses simple percent-encoding of
    /// key and value via `url::form_urlencoded`.
    pub fn query(mut self, params: &[(&str, &str)]) -> Self {
        if params.is_empty() {
            return self;
        }
        let encoded = params
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        let separator = if self.url.contains('?') { '&' } else { '?' };
        self.url.push(separator);
        self.url.push_str(&encoded);
        self
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    pub final_url: String,
    pub elapsed_ms: u64,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn text(&self) -> io::Result<String> {
        String::from_utf8(self.body.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn content_type(&self) -> Option<&str> {
        self.headers.get("content-type").map(|s| s.as_str())
    }
}

/// Execute an HTTP request via the host's `http.fetch` capability.
pub fn fetch(request: &Request) -> io::Result<Response> {
    let body_b64 = request.body.as_ref().map(|b| protocol::base64_encode(b));
    let params = json!({
        "url": request.url,
        "method": request.method,
        "headers": request.headers,
        "body": body_b64,
        "timeout_ms": request.timeout_ms,
    });

    let data = crate::runtime::capability_call("http", "fetch", params)?;

    let status = data
        .get("status")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "http.fetch response missing status",
            )
        })? as u16;

    let headers = data
        .get("headers")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let body = data
        .get("body")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(protocol::base64_decode)
        .transpose()?
        .unwrap_or_default();

    let final_url = data
        .get("final_url")
        .and_then(|v| v.as_str())
        .unwrap_or(&request.url)
        .to_string();

    let elapsed_ms = data.get("elapsed_ms").and_then(|v| v.as_u64()).unwrap_or(0);

    Ok(Response {
        status,
        headers,
        body,
        final_url,
        elapsed_ms,
    })
}

/// Convenience: GET the URL with default options, return the full response.
pub fn get(url: impl Into<String>) -> io::Result<Response> {
    fetch(&Request::get(url))
}
