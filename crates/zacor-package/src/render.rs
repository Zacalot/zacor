//! Rendering helpers backed by the installable `zr-mermaid` package.

use std::collections::BTreeMap;
use std::io;

/// Render a mermaid diagram to SVG via the host's `render.mermaid` capability.
/// Returns the SVG text.
pub fn mermaid(source: &str) -> io::Result<String> {
    let mut args = BTreeMap::new();
    args.insert("source".to_string(), source.to_string());
    let mut records = crate::invoke("zr-mermaid", "render", &args)?;
    records
        .next()
        .and_then(|value| value.get("svg").and_then(|v| v.as_str()).map(String::from))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unexpected response format for zr-mermaid render"))
}
