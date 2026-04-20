//! Rendering capabilities — currently mermaid → SVG.
//!
//! The host owns the mermaid renderer (ships as a heavy dep tree), so this
//! module just shapes the request and calls the `render.mermaid` capability.

use serde_json::json;
use std::io;

/// Render a mermaid diagram to SVG via the host's `render.mermaid` capability.
/// Returns the SVG text.
pub fn mermaid(source: &str) -> io::Result<String> {
    let data = crate::runtime::capability_call("render", "mermaid", json!({"source": source}))?;
    data.get("svg")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected response format for render.mermaid",
            )
        })
}
