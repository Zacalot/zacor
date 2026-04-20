//! Prompt capability — interactive user prompts via the protocol.
//!
//! Only available when the protocol runtime is active (ProtocolLocal or
//! ProtocolRemote mode). Returns an error in Local (library) mode.

use super::ExecMode;
use serde_json::json;
use std::io;

fn require_protocol() -> io::Result<()> {
    if super::mode() == ExecMode::Local {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "prompt not available in this context",
        ));
    }
    Ok(())
}

/// Ask the user a yes/no question. Returns true for yes, false for no.
pub fn confirm(message: &str) -> io::Result<bool> {
    require_protocol()?;
    let data = crate::runtime::capability_call("prompt", "confirm", json!({"message": message}))?;
    data.get("answer").and_then(|v| v.as_bool()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected response format for prompt.confirm",
        )
    })
}

/// Ask the user to choose from a list of options. Returns the chosen option string.
pub fn choose(message: &str, options: &[&str]) -> io::Result<String> {
    require_protocol()?;
    let data = crate::runtime::capability_call(
        "prompt",
        "choose",
        json!({"message": message, "options": options}),
    )?;
    data.get("answer")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected response format for prompt.choose",
            )
        })
}

/// Ask the user for free-form text input. Returns the entered text.
pub fn text(message: &str) -> io::Result<String> {
    require_protocol()?;
    let data = crate::runtime::capability_call("prompt", "text", json!({"message": message}))?;
    data.get("answer")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected response format for prompt.text",
            )
        })
}
