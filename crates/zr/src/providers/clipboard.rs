use super::invalid_input;
use serde_json::json;
use zacor_host::capability::CapabilityProvider;
use zacor_host::protocol::CapabilityError;

pub struct ClipboardProvider;

impl CapabilityProvider for ClipboardProvider {
    fn domain(&self) -> &str {
        "clipboard"
    }

    fn handle(
        &self,
        op: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, CapabilityError> {
        match op {
            "read" => {
                let mut clipboard = arboard::Clipboard::new()
                    .map_err(|error| CapabilityError::from_io(&std::io::Error::other(error.to_string())))?;
                let text = clipboard
                    .get_text()
                    .map_err(|error| CapabilityError::from_io(&std::io::Error::other(error.to_string())))?;
                Ok(json!({"text": text}))
            }
            "write" => {
                let text = params.get("text").and_then(|value| value.as_str()).unwrap_or("");
                let mut clipboard = arboard::Clipboard::new()
                    .map_err(|error| CapabilityError::from_io(&std::io::Error::other(error.to_string())))?;
                clipboard
                    .set_text(text.to_string())
                    .map_err(|error| CapabilityError::from_io(&std::io::Error::other(error.to_string())))?;
                Ok(json!({}))
            }
            _ => Err(invalid_input(format!("unknown clipboard operation: {op}"))),
        }
    }
}
