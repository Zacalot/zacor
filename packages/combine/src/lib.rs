use serde_json::{json, Map, Value};
use std::io::BufRead;

zacor_package::include_args!();

fn load_second_source(file: &str, records: &str) -> Result<Vec<Value>, String> {
    if !file.is_empty() {
        let content = std::fs::read_to_string(file)
            .map_err(|e| format!("combine: cannot read file '{file}': {e}"))?;
        parse_string(&content)
    } else if !records.is_empty() {
        parse_string(records)
    } else {
        Err("combine: requires --file or --records".into())
    }
}

fn load_file_source(file: &str) -> Result<Vec<Value>, String> {
    if file.is_empty() {
        return Err("combine: requires --file".into());
    }
    let content = std::fs::read_to_string(file)
        .map_err(|e| format!("combine: cannot read file '{file}': {e}"))?;
    parse_string(&content)
}

fn parse_string(s: &str) -> Result<Vec<Value>, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(trimmed) {
        return Ok(arr);
    }

    let mut records = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let val: Value = serde_json::from_str(line)
            .map_err(|e| format!("combine: invalid JSON: {e}"))?;
        match val {
            Value::Array(arr) => records.extend(arr),
            other => records.push(other),
        }
    }
    Ok(records)
}

pub fn cmd_append(
    args: &args::AppendArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("combine append: requires piped input")?;
    let mut records = zacor_package::parse_records(reader)?;
    let second = load_second_source(args.file.as_deref().unwrap_or(""), args.records.as_deref().unwrap_or(""))?;
    records.extend(second);
    Ok(records)
}

pub fn cmd_prepend(
    args: &args::PrependArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("combine prepend: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;
    let mut second = load_second_source(args.file.as_deref().unwrap_or(""), args.records.as_deref().unwrap_or(""))?;
    second.extend(records);
    Ok(second)
}

pub fn cmd_merge(
    args: &args::MergeArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("combine merge: requires piped input")?;
    let left = zacor_package::parse_records(reader)?;
    let right = load_file_source(args.file.as_deref().unwrap_or(""))?;

    let output: Vec<Value> = left.into_iter().enumerate().map(|(i, record)| {
        if let Value::Object(mut map) = record {
            if let Some(Value::Object(rmap)) = right.get(i) {
                for (k, v) in rmap {
                    map.insert(k.clone(), v.clone());
                }
            }
            Value::Object(map)
        } else {
            record
        }
    }).collect();

    Ok(output)
}

pub fn cmd_join(
    args: &args::JoinArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("combine join: requires piped input")?;
    let left = zacor_package::parse_records(reader)?;
    let right = load_file_source(args.file.as_deref().unwrap_or(""))?;

    let left_key = &args.left_key;
    let rk_str = args.right_key.as_deref().unwrap_or("").to_string();
    let right_key: &str = if rk_str.is_empty() { left_key } else { &rk_str };
    let prefix: &str = args.prefix.as_deref().unwrap_or("");

    let is_left = args.left;
    let is_right = args.right;
    let is_outer = args.outer;

    // Build right index
    let right_index: std::collections::HashMap<String, Vec<&Value>> = {
        let mut idx: std::collections::HashMap<String, Vec<&Value>> = std::collections::HashMap::new();
        for r in &right {
            let key = r.as_object()
                .and_then(|m| m.get(right_key))
                .map(|v| value_to_key(v))
                .unwrap_or_default();
            idx.entry(key).or_default().push(r);
        }
        idx
    };

    let mut output = Vec::new();
    let mut right_matched: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for lrec in &left {
        let lkey = lrec.as_object()
            .and_then(|m| m.get(left_key.as_str()))
            .map(|v| value_to_key(v))
            .unwrap_or_default();

        if let Some(matches) = right_index.get(&lkey) {
            for &rrec in matches {
                let merged = merge_records(lrec, rrec, right_key, prefix);
                output.push(merged);
                // Track matched right records
                for (i, r) in right.iter().enumerate() {
                    let rk = r.as_object()
                        .and_then(|m| m.get(right_key))
                        .map(|v| value_to_key(v))
                        .unwrap_or_default();
                    if rk == lkey {
                        right_matched.insert(i);
                    }
                }
            }
        } else if is_left || is_outer {
            output.push(lrec.clone());
        }
    }

    if is_right || is_outer {
        for (i, rrec) in right.iter().enumerate() {
            if !right_matched.contains(&i) {
                output.push(rrec.clone());
            }
        }
    }

    Ok(output)
}

fn merge_records(left: &Value, right: &Value, right_key: &str, prefix: &str) -> Value {
    let mut result = match left {
        Value::Object(m) => m.clone(),
        _ => Map::new(),
    };

    if let Value::Object(rmap) = right {
        for (k, v) in rmap {
            if k == right_key {
                continue; // Skip the join key from right side
            }
            let key = if !prefix.is_empty() && result.contains_key(k) {
                format!("{prefix}{k}")
            } else {
                k.clone()
            };
            result.insert(key, v.clone());
        }
    }

    Value::Object(result)
}

fn value_to_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

pub fn cmd_zip(
    args: &args::ZipArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("combine zip: requires piped input")?;
    let left = zacor_package::parse_records(reader)?;
    let right = load_file_source(args.file.as_deref().unwrap_or(""))?;

    let len = left.len().min(right.len());
    let output: Vec<Value> = left.into_iter().zip(right.into_iter())
        .take(len)
        .map(|(l, r)| json!({"left": l, "right": r}))
        .collect();

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;
    use zacor_package::FromArgs;
    use std::collections::BTreeMap;

    fn make_args<T: FromArgs>(pairs: &[(&str, Value)]) -> T {
        let map: BTreeMap<String, Value> = pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        T::from_args(&map).unwrap()
    }

    fn json_input(s: &str) -> Option<Box<dyn BufRead>> {
        Some(Box::new(Cursor::new(s.to_string().into_bytes())))
    }

    #[test]
    fn append_from_records() {
        let data = r#"[{"a":1}]"#;
        let args: args::AppendArgs = make_args(&[("records", json!(r#"[{"a":2},{"a":3}]"#))]);
        let result = cmd_append(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["a"], 1);
        assert_eq!(result[1]["a"], 2);
    }

    #[test]
    fn prepend_from_records() {
        let data = r#"[{"a":3}]"#;
        let args: args::PrependArgs = make_args(&[("records", json!(r#"[{"a":1},{"a":2}]"#))]);
        let result = cmd_prepend(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["a"], 1);
        assert_eq!(result[2]["a"], 3);
    }

    #[test]
    fn no_source_error() {
        let data = r#"[{"a":1}]"#;
        let args: args::AppendArgs = make_args(&[]);
        assert!(cmd_append(&args, json_input(data)).is_err());
    }

    // Merge, join, and zip tests require file-based input.
    // We test the parsing and logic via helper functions.

    #[test]
    fn parse_string_json_array() {
        let records = parse_string(r#"[{"a":1},{"a":2}]"#).unwrap();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn parse_string_jsonl() {
        let records = parse_string("{\"a\":1}\n{\"a\":2}").unwrap();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn merge_records_fn() {
        let left = json!({"id": 1, "name": "a"});
        let right = json!({"id": 1, "score": 90});
        let result = super::merge_records(&left, &right, "id", "");
        assert_eq!(result["name"], "a");
        assert_eq!(result["score"], 90);
        assert_eq!(result["id"], 1);
    }

    #[test]
    fn merge_records_prefix() {
        let left = json!({"id": 1, "name": "a"});
        let right = json!({"id": 1, "name": "b"});
        let result = super::merge_records(&left, &right, "id", "right_");
        assert_eq!(result["name"], "a");
        assert_eq!(result["right_name"], "b");
    }
}
