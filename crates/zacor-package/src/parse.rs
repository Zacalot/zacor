//! Code-parsing helpers backed by the installable `treesitter` package.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    let mut args = BTreeMap::new();
    args.insert("source".to_string(), source.to_string());
    args.insert("ext".to_string(), ext.to_string());
    args.insert("rel_path".to_string(), rel_path.to_string());

    crate::invoke("treesitter", "parse", &args)?
        .map(|value| {
            serde_json::from_value(value).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("decl parse: {e}"))
            })
        })
        .collect()
}
