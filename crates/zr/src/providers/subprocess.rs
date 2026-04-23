use super::{invalid_input, resolve_path};
use serde_json::json;
use std::io::Write;
use std::process::{Command, Stdio};
use zacor_host::capability::CapabilityProvider;
use zacor_host::protocol::{self, CapabilityError};

pub struct SubprocessProvider;

impl CapabilityProvider for SubprocessProvider {
    fn domain(&self) -> &str {
        "subprocess"
    }

    fn handle(
        &self,
        op: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, CapabilityError> {
        match op {
            "exec" => exec(params),
            _ => Err(invalid_input(format!("unknown subprocess operation: {op}"))),
        }
    }
}

fn exec(params: &serde_json::Value) -> Result<serde_json::Value, CapabilityError> {
    let command = params
        .get("command")
        .and_then(|value| value.as_str())
        .ok_or_else(|| invalid_input("subprocess.exec: command is required"))?;
    let args = params
        .get("args")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut child = Command::new(command);
    child
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(cwd) = params.get("cwd").and_then(|value| value.as_str())
        && !cwd.is_empty()
    {
        child.current_dir(resolve_path(cwd));
    }

    if let Some(env) = params.get("env").and_then(|value| value.as_object()) {
        child.envs(env.iter().filter_map(|(key, value)| {
            value.as_str().map(|value| (key.clone(), value.to_string()))
        }));
    }

    let stdin_bytes = match params.get("stdin").and_then(|value| value.as_str()) {
        Some(stdin) if !stdin.is_empty() => Some(protocol::base64_decode(stdin).map_err(|error| CapabilityError::from_io(&error))?),
        _ => None,
    };

    let mut child = child.spawn().map_err(|error| CapabilityError::from_io(&error))?;
    if let Some(stdin_bytes) = stdin_bytes
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin
            .write_all(&stdin_bytes)
            .map_err(|error| CapabilityError::from_io(&error))?;
    }

    let output = child.wait_with_output().map_err(|error| CapabilityError::from_io(&error))?;
    Ok(json!({
        "exit_code": output.status.code().unwrap_or(-1),
        "stdout_base64": protocol::base64_encode(&output.stdout),
        "stderr_base64": protocol::base64_encode(&output.stderr),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_returns_status_and_output() {
        let data = SubprocessProvider
            .handle("exec", &json!({"command": "cargo", "args": ["--version"]}))
            .unwrap();

        assert_eq!(data["exit_code"], 0);
        let stdout = protocol::base64_decode(data["stdout_base64"].as_str().unwrap()).unwrap();
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("cargo"), "got: {stdout}");
    }

    #[test]
    fn unknown_operation_returns_invalid_input() {
        let error = SubprocessProvider.handle("spawn", &json!({})).unwrap_err();
        assert_eq!(error.kind, "invalid_input");
        assert_eq!(error.message, "unknown subprocess operation: spawn");
    }
}
