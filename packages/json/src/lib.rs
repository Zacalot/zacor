use serde::Serialize;
use std::io::{BufRead, Read};

zacor_package::include_args!();

#[derive(Serialize)]
pub struct JsonRecord {
    pub output: String,
    pub valid: bool,
}

pub fn json(
    indent: usize,
    compact: bool,
    validate: bool,
    input: Box<dyn BufRead>,
) -> Result<JsonRecord, String> {
    let mut raw = String::new();
    let mut reader = input;
    reader
        .read_to_string(&mut raw)
        .map_err(|e| format!("json: read error: {e}"))?;

    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("json: invalid JSON: {e}"))?;

    if validate {
        return Ok(JsonRecord {
            output: String::new(),
            valid: true,
        });
    }

    let output = if compact {
        serde_json::to_string(&parsed).map_err(|e| format!("json: serialize: {e}"))?
    } else {
        let indent_str = " ".repeat(indent);
        let formatter = serde_json::ser::PrettyFormatter::with_indent(indent_str.as_bytes());
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        serde::Serialize::serialize(&parsed, &mut ser)
            .map_err(|e| format!("json: serialize: {e}"))?;
        String::from_utf8(buf).map_err(|e| format!("json: utf8: {e}"))?
    };

    Ok(JsonRecord {
        output,
        valid: true,
    })
}
