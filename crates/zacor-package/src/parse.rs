//! Code-parsing capabilities — currently tree-sitter-backed signature extraction.
//!
//! The host owns the tree-sitter grammars (they ship as C bindings and don't
//! cross-compile to `wasm32-wasip1`), so this module just shapes the request
//! and calls the `parse.tree-sitter` capability.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io;

/// A single extracted declaration. Mirrors the host-side shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Declaration {
    pub file: String,
    pub kind: String,
    pub name: String,
    pub signature: String,
}

/// Extract declarations from a source file's contents using the host's
/// tree-sitter grammars. Returns an empty list when the extension isn't
/// one of the supported languages (rs, ts, tsx, js, jsx, mjs, cjs, py).
pub fn tree_sitter(source: &str, ext: &str, rel_path: &str) -> io::Result<Vec<Declaration>> {
    let data = crate::runtime::capability_call(
        "parse",
        "tree-sitter",
        json!({
            "source": source,
            "ext": ext,
            "rel_path": rel_path,
        }),
    )?;

    let arr = data
        .get("declarations")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected response format for parse.tree-sitter",
            )
        })?;

    arr.iter()
        .map(|v| {
            serde_json::from_value(v.clone()).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("decl parse: {e}"))
            })
        })
        .collect()
}
