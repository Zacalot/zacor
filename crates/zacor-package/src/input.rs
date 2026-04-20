use serde_json::Value;
use std::io::BufRead;

/// Parse a buffered reader of JSON records into a `Vec<Value>`.
///
/// Tries JSON array first; falls back to line-delimited JSON (JSONL).
/// In JSONL mode, lines containing arrays are flattened one level.
/// Blank lines are skipped.
pub fn parse_records(input: Box<dyn BufRead>) -> Result<Vec<Value>, String> {
    let mut lines = Vec::new();
    for line in input.lines() {
        let line = line.map_err(|e| format!("input: read error: {e}"))?;
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() {
            lines.push(trimmed);
        }
    }

    let all = lines.join("\n");

    if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&all) {
        return Ok(arr);
    }

    let mut records = Vec::new();
    for line in &lines {
        let val: Value =
            serde_json::from_str(line).map_err(|e| format!("input: invalid JSON: {e}"))?;
        match val {
            Value::Array(arr) => records.extend(arr),
            other => records.push(other),
        }
    }
    Ok(records)
}

/// Split a field specification string into individual field names.
///
/// Accepts comma-separated, space-separated, or mixed separators.
/// Empty segments are filtered out.
pub fn parse_field_list(fields: &str) -> Vec<&str> {
    fields
        .split_whitespace()
        .flat_map(|s| s.split(','))
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn input(s: &str) -> Box<dyn BufRead> {
        Box::new(Cursor::new(s.to_string().into_bytes()))
    }

    // ─── parse_records tests ────────────────────────────────────────

    #[test]
    fn json_array() {
        let result = parse_records(input(r#"[{"a":1},{"a":2}]"#)).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["a"], 1);
        assert_eq!(result[1]["a"], 2);
    }

    #[test]
    fn jsonl() {
        let result = parse_records(input("{\"a\":1}\n{\"a\":2}\n{\"a\":3}")).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["a"], 1);
        assert_eq!(result[2]["a"], 3);
    }

    #[test]
    fn nested_array_flattening() {
        let result = parse_records(input("[{\"a\":1},{\"a\":2}]\n{\"a\":3}")).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn empty_input() {
        let result = parse_records(input("")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn invalid_json() {
        let result = parse_records(input("not json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid JSON"));
    }

    #[test]
    fn mixed_whitespace() {
        let result = parse_records(input("\n  {\"a\":1}  \n\n  {\"a\":2}  \n")).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["a"], 1);
        assert_eq!(result[1]["a"], 2);
    }

    // ─── parse_field_list tests ─────────────────────────────────────

    #[test]
    fn space_separated() {
        assert_eq!(
            parse_field_list("name kind size"),
            vec!["name", "kind", "size"]
        );
    }

    #[test]
    fn comma_separated() {
        assert_eq!(
            parse_field_list("name,kind,size"),
            vec!["name", "kind", "size"]
        );
    }

    #[test]
    fn mixed_separators() {
        assert_eq!(
            parse_field_list("name,kind size"),
            vec!["name", "kind", "size"]
        );
    }

    #[test]
    fn empty_input_fields() {
        assert!(parse_field_list("").is_empty());
        assert!(parse_field_list("   ").is_empty());
    }

    #[test]
    fn consecutive_separators() {
        assert_eq!(parse_field_list("name,,kind"), vec!["name", "kind"]);
        assert_eq!(parse_field_list("name,  ,kind"), vec!["name", "kind"]);
    }
}
