use crate::provider_support::invalid_input;
use serde_json::json;
use std::time::Duration;
use zacor_host::capability::CapabilityProvider;
use zacor_host::protocol::{self, CapabilityError};

pub(super) struct HttpProvider {
    client: reqwest::blocking::Client,
}

impl HttpProvider {
    pub(super) fn new() -> Self {
        Self {
            client: reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()
                .expect("http client should build"),
        }
    }
}

impl CapabilityProvider for HttpProvider {
    fn domain(&self) -> &str {
        "http"
    }

    fn handle(
        &self,
        op: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, CapabilityError> {
        match op {
            "fetch" => self.fetch(params),
            _ => Err(invalid_input(format!("unknown http operation: {op}"))),
        }
    }
}

impl HttpProvider {
    fn fetch(&self, params: &serde_json::Value) -> Result<serde_json::Value, CapabilityError> {
        let url = params
            .get("url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| invalid_input("http.fetch: url is required"))?;
        let method = params
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or("GET")
            .to_ascii_uppercase();
        let timeout_ms = params
            .get("timeout_ms")
            .and_then(|value| value.as_u64())
            .unwrap_or(30_000);
        let body = match params.get("body").and_then(|value| value.as_str()) {
            Some(body) if !body.is_empty() => Some(
                protocol::base64_decode(body).map_err(|error| CapabilityError::from_io(&error))?,
            ),
            _ => None,
        };

        let method: reqwest::Method = method
            .parse()
            .map_err(|error| invalid_input(format!("invalid method '{}': {}", method, error)))?;

        let mut builder = self
            .client
            .request(method, url)
            .timeout(Duration::from_millis(timeout_ms));
        if let Some(headers) = params.get("headers").and_then(|value| value.as_object()) {
            for (name, value) in headers {
                if let Some(value) = value.as_str() {
                    builder = builder.header(name, value);
                }
            }
        }
        if let Some(body) = body {
            builder = builder.body(body);
        }

        let start = std::time::Instant::now();
        let response = builder
            .send()
            .map_err(|error| CapabilityError::from_io(&std::io::Error::other(error)))?;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let mut headers = serde_json::Map::new();
        for (name, value) in response.headers() {
            if let Ok(value) = value.to_str() {
                headers.insert(name.as_str().to_string(), json!(value));
            }
        }
        let body = response
            .bytes()
            .map_err(|error| CapabilityError::from_io(&std::io::Error::other(error)))?;

        Ok(json!({
            "status": status,
            "headers": headers,
            "body": protocol::base64_encode(&body),
            "final_url": final_url,
            "elapsed_ms": elapsed_ms,
        }))
    }
}
