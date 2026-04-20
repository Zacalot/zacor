use serde_json::{json, Map, Value};
use std::io::BufRead;

zacor_package::include_args!();

pub fn cmd_rename(
    args: &args::RenameArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("reshape rename: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let mapping: Map<String, Value> = serde_json::from_str(&args.column)
        .map_err(|e| format!("reshape rename: invalid column mapping JSON: {e}"))?;

    let rename_map: Vec<(String, String)> = mapping.into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect();

    let output: Vec<Value> = records.into_iter().map(|record| {
        if let Value::Object(mut map) = record {
            for (old, new) in &rename_map {
                if let Some(val) = map.remove(old) {
                    map.insert(new.clone(), val);
                }
            }
            Value::Object(map)
        } else {
            record
        }
    }).collect();

    Ok(output)
}

pub fn cmd_flatten(
    args: &args::FlattenArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("reshape flatten: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let fields: Vec<&str> = args.fields.as_deref()
        .map(|s| zacor_package::parse_field_list(s))
        .unwrap_or_default();
    let recursive = args.all;

    let output: Vec<Value> = records.into_iter().map(|record| {
        if let Value::Object(map) = record {
            let mut result = Map::new();
            for (k, v) in map {
                let should_flatten = fields.is_empty() || fields.contains(&k.as_str());
                if should_flatten {
                    if let Value::Object(inner) = v {
                        flatten_into(&mut result, &k, inner, recursive);
                    } else {
                        result.insert(k, v);
                    }
                } else {
                    result.insert(k, v);
                }
            }
            Value::Object(result)
        } else {
            record
        }
    }).collect();

    Ok(output)
}

fn flatten_into(target: &mut Map<String, Value>, prefix: &str, map: Map<String, Value>, recursive: bool) {
    for (k, v) in map {
        let key = format!("{prefix}_{k}");
        if recursive {
            if let Value::Object(inner) = v {
                flatten_into(target, &key, inner, true);
                continue;
            }
        }
        target.insert(key, v);
    }
}

pub fn cmd_transpose(
    args: &args::TransposeArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("reshape transpose: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let names: Vec<&str> = args.names.as_deref()
        .map(|s| s.split_whitespace().collect())
        .unwrap_or_else(|| vec!["key", "value"]);
    let key_name = names.first().copied().unwrap_or("key");
    let val_prefix = if names.len() > 1 { &names[1..] } else { &["value"] };

    if records.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all field names from first record
    let field_names: Vec<String> = if let Value::Object(map) = &records[0] {
        map.keys().cloned().collect()
    } else {
        return Ok(Vec::new());
    };

    let mut output = Vec::new();
    for field in &field_names {
        let mut row = Map::new();
        row.insert(key_name.to_string(), json!(field));
        for (i, record) in records.iter().enumerate() {
            let col_name = if val_prefix.len() == 1 && records.len() == 1 {
                val_prefix[0].to_string()
            } else {
                format!("{}_{}", val_prefix.first().unwrap_or(&"value"), i)
            };
            let val = record.as_object()
                .and_then(|m| m.get(field))
                .cloned()
                .unwrap_or(Value::Null);
            row.insert(col_name, val);
        }
        output.push(Value::Object(row));
    }

    Ok(output)
}

pub fn cmd_wrap(
    args: &args::WrapArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("reshape wrap: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let output: Vec<Value> = records.into_iter().map(|v| {
        json!({ &args.name: v })
    }).collect();

    Ok(output)
}

pub fn cmd_group_by(
    args: &args::GroupByArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("reshape group-by: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let fields = zacor_package::parse_field_list(&args.fields);

    if fields.len() == 1 {
        let field = fields[0];
        let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
        let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for record in records {
            let key = record.as_object()
                .and_then(|m| m.get(field))
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| "null".to_string());

            if let Some(&idx) = index.get(&key) {
                groups[idx].1.push(record);
            } else {
                index.insert(key.clone(), groups.len());
                groups.push((key, vec![record]));
            }
        }

        if args.to_table {
            Ok(groups.into_iter().map(|(key, items)| {
                json!({"group": key, "items": items})
            }).collect())
        } else {
            let mut map = Map::new();
            for (key, items) in groups {
                map.insert(key, Value::Array(items));
            }
            Ok(vec![Value::Object(map)])
        }
    } else {
        // Multi-field: nest hierarchically
        // For simplicity, treat multi-field as concatenated key
        let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
        let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for record in records {
            let key_parts: Vec<String> = fields.iter().map(|&f| {
                record.as_object()
                    .and_then(|m| m.get(f))
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_else(|| "null".to_string())
            }).collect();
            let key = key_parts.join("\0");

            if let Some(&idx) = index.get(&key) {
                groups[idx].1.push(record);
            } else {
                index.insert(key.clone(), groups.len());
                groups.push((key, vec![record]));
            }
        }

        if args.to_table {
            Ok(groups.into_iter().map(|(_, items)| {
                let group_val = fields.iter().map(|&f| {
                    items[0].as_object()
                        .and_then(|m| m.get(f))
                        .map(|v| match v {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_else(|| "null".to_string())
                }).collect::<Vec<_>>().join(", ");
                json!({"group": group_val, "items": items})
            }).collect())
        } else {
            // Build nested structure
            build_nested_groups(&records_placeholder_unused(), &fields)
        }
    }
}

fn build_nested_groups(_records: &[Value], _fields: &[&str]) -> Result<Vec<Value>, String> {
    // Simplified: for multi-field, use to-table mode internally
    Err("reshape group-by: multi-field grouping requires --to-table flag".into())
}

fn records_placeholder_unused() -> Vec<Value> {
    Vec::new()
}

pub fn cmd_enumerate(
    _args: &args::EnumerateArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("reshape enumerate: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let output: Vec<Value> = records.into_iter().enumerate().map(|(i, record)| {
        if let Value::Object(map) = record {
            // Insert index at front
            let mut new_map = Map::new();
            new_map.insert("index".to_string(), json!(i));
            new_map.extend(map);
            Value::Object(new_map)
        } else {
            json!({"index": i, "value": record})
        }
    }).collect();

    Ok(output)
}

pub fn cmd_columns(
    _args: &args::ColumnsArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("reshape columns: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    if records.is_empty() {
        return Ok(Vec::new());
    }

    if let Value::Object(map) = &records[0] {
        Ok(map.keys().map(|k| json!({"value": k})).collect())
    } else {
        Ok(Vec::new())
    }
}

pub fn cmd_values(
    _args: &args::ValuesArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("reshape values: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let mut output = Vec::new();
    for record in records {
        if let Value::Object(map) = record {
            for (_, v) in map {
                output.push(json!({"value": v}));
            }
        }
    }
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
    fn rename_fields() {
        let data = r#"[{"old_name":"hello","keep":"x"}]"#;
        let args: args::RenameArgs = make_args(&[("column", json!(r#"{"old_name":"new_name"}"#))]);
        let result = cmd_rename(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["new_name"], "hello");
        assert!(result[0].get("old_name").is_none());
        assert_eq!(result[0]["keep"], "x");
    }

    #[test]
    fn rename_missing_field() {
        let data = r#"[{"a":1}]"#;
        let args: args::RenameArgs = make_args(&[("column", json!(r#"{"missing":"new"}"#))]);
        let result = cmd_rename(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["a"], 1);
    }

    #[test]
    fn flatten_nested() {
        let data = r#"[{"a":1,"b":{"c":2,"d":3}}]"#;
        let args: args::FlattenArgs = make_args(&[]);
        let result = cmd_flatten(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["a"], 1);
        assert_eq!(result[0]["b_c"], 2);
        assert_eq!(result[0]["b_d"], 3);
    }

    #[test]
    fn flatten_specific_fields() {
        let data = r#"[{"a":1,"b":{"c":2},"d":{"e":3}}]"#;
        let args: args::FlattenArgs = make_args(&[("fields", json!("b"))]);
        let result = cmd_flatten(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["b_c"], 2);
        assert!(result[0].get("d").is_some()); // d should remain nested
    }

    #[test]
    fn flatten_recursive() {
        let data = r#"[{"a":{"b":{"c":1}}}]"#;
        let args: args::FlattenArgs = make_args(&[("all", json!(true))]);
        let result = cmd_flatten(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["a_b_c"], 1);
    }

    #[test]
    fn transpose_basic() {
        let data = r#"[{"a":1,"b":2}]"#;
        let args: args::TransposeArgs = make_args(&[]);
        let result = cmd_transpose(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["key"], "a");
    }

    #[test]
    fn wrap_values() {
        let data = r#"[1,2,3]"#;
        let args: args::WrapArgs = make_args(&[("name", json!("id"))]);
        let result = cmd_wrap(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["id"], 1);
        assert_eq!(result[2]["id"], 3);
    }

    #[test]
    fn group_by_field() {
        let data = r#"[{"type":"a","v":1},{"type":"b","v":2},{"type":"a","v":3}]"#;
        let args: args::GroupByArgs = make_args(&[("fields", json!("type"))]);
        let result = cmd_group_by(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 1);
        let map = result[0].as_object().unwrap();
        assert_eq!(map["a"].as_array().unwrap().len(), 2);
        assert_eq!(map["b"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn group_by_to_table() {
        let data = r#"[{"type":"a","v":1},{"type":"b","v":2},{"type":"a","v":3}]"#;
        let args: args::GroupByArgs = make_args(&[("fields", json!("type")), ("to-table", json!(true))]);
        let result = cmd_group_by(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["group"], "a");
        assert_eq!(result[0]["items"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn enumerate_records() {
        let data = r#"[{"name":"a"},{"name":"b"}]"#;
        let args: args::EnumerateArgs = make_args(&[]);
        let result = cmd_enumerate(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["index"], 0);
        assert_eq!(result[0]["name"], "a");
        assert_eq!(result[1]["index"], 1);
    }

    #[test]
    fn columns_from_record() {
        let data = r#"[{"name":"a","size":1,"type":"f"}]"#;
        let args: args::ColumnsArgs = make_args(&[]);
        let result = cmd_columns(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 3);
        let vals: Vec<&str> = result.iter().map(|r| r["value"].as_str().unwrap()).collect();
        assert!(vals.contains(&"name"));
        assert!(vals.contains(&"size"));
        assert!(vals.contains(&"type"));
    }

    #[test]
    fn columns_empty() {
        let args: args::ColumnsArgs = make_args(&[]);
        let result = cmd_columns(&args, json_input("[]")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn values_from_records() {
        let data = r#"[{"a":1,"b":2}]"#;
        let args: args::ValuesArgs = make_args(&[]);
        let result = cmd_values(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 2);
    }
}
