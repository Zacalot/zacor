use super::invalid_input;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use zacor_host::capability::CapabilityProvider;
use zacor_host::protocol::{CapabilityError, CapabilityResult};

pub struct HttpForwarder;

impl CapabilityProvider for HttpForwarder {
    fn domain(&self) -> &str {
        "http"
    }

    fn handle(&self, op: &str, params: &Value) -> Result<Value, CapabilityError> {
        let home = crate::paths::zr_home().map_err(|error| {
            CapabilityError::from_io(&std::io::Error::other(error.to_string()))
        })?;
        let stream = crate::daemon_client::connect_or_start_daemon(&home).map_err(|error| {
            CapabilityError::from_io(&std::io::Error::other(error.to_string()))
        })?;

        let req = serde_json::json!({
            "request": "capability-forward",
            "domain": "http",
            "op": op,
            "params": params,
        });

        let mut writer = stream.try_clone().map_err(|error| CapabilityError::from_io(&error))?;
        writeln!(writer, "{}", req).map_err(|error| CapabilityError::from_io(&error))?;
        writer.flush().map_err(|error| CapabilityError::from_io(&error))?;

        let mut line = String::new();
        BufReader::new(stream)
            .read_line(&mut line)
            .map_err(|error| CapabilityError::from_io(&error))?;
        let response: serde_json::Value = serde_json::from_str(line.trim())
            .map_err(|error| invalid_input(format!("invalid daemon response: {error}")))?;

        if !response["ok"].as_bool().unwrap_or(false) {
            return Err(response
                .get("error")
                .cloned()
                .and_then(|value| serde_json::from_value::<CapabilityError>(value).ok())
                .unwrap_or_else(|| invalid_input("daemon capability-forward failed")));
        }

        let capability_res = response
            .get("result")
            .cloned()
            .and_then(|value| serde_json::from_value::<zacor_host::protocol::CapabilityRes>(value).ok())
            .ok_or_else(|| invalid_input("daemon capability-forward missing result"))?;

        match capability_res.result {
            CapabilityResult::Ok { data } => Ok(data),
            CapabilityResult::Error { error } => Err(error),
        }
    }
}
