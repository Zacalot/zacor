use serde_json::{json, Map, Value};
use std::io::BufRead;

zacor_package::include_args!();

pub fn cmd_skip(
    args: &args::SkipArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("filter skip: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;
    let n = args.count.max(0) as usize;
    Ok(records.into_iter().skip(n).collect())
}

pub fn cmd_drop(
    args: &args::DropArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("filter drop: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;
    let n = args.count.max(0) as usize;
    let len = records.len();
    let take = len.saturating_sub(n);
    Ok(records.into_iter().take(take).collect())
}

pub fn cmd_uniq(
    args: &args::UniqArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("filter uniq: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    if records.is_empty() {
        return Ok(Vec::new());
    }

    // Group consecutive duplicates
    let mut groups: Vec<(Value, usize)> = Vec::new();
    for record in records {
        let matches_prev = if let Some((prev, _)) = groups.last() {
            if args.ignore_case {
                values_equal_ignore_case(prev, &record)
            } else {
                prev == &record
            }
        } else {
            false
        };

        if matches_prev {
            groups.last_mut().unwrap().1 += 1;
        } else {
            groups.push((record, 1));
        }
    }

    let mut output = Vec::new();
    for (record, count) in groups {
        if args.repeated && count < 2 {
            continue;
        }
        if args.unique && count > 1 {
            continue;
        }

        if args.count {
            let mut map = match record {
                Value::Object(m) => m,
                other => {
                    let mut m = Map::new();
                    m.insert("value".to_string(), other);
                    m
                }
            };
            map.insert("count".to_string(), json!(count));
            output.push(Value::Object(map));
        } else {
            output.push(record);
        }
    }

    Ok(output)
}

pub fn cmd_uniq_by(
    args: &args::UniqByArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("filter uniq-by: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let fields = zacor_package::parse_field_list(&args.fields);

    if fields.is_empty() {
        return Err("filter uniq-by: at least one field is required".into());
    }

    if args.keep_last {
        // Keep last occurrence: iterate in reverse, dedup, then reverse
        let mut seen = std::collections::HashSet::new();
        let mut output: Vec<Value> = Vec::new();
        for record in records.into_iter().rev() {
            let key = extract_key(&record, &fields);
            if seen.insert(key) {
                output.push(record);
            }
        }
        output.reverse();
        Ok(output)
    } else {
        let mut seen = std::collections::HashSet::new();
        let mut output = Vec::new();
        for record in records {
            let key = extract_key(&record, &fields);
            if seen.insert(key) {
                output.push(record);
            }
        }
        Ok(output)
    }
}

pub fn cmd_compact(
    args: &args::CompactArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("filter compact: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let fields: Vec<&str> = args.fields.as_deref()
        .map(|s| zacor_package::parse_field_list(s))
        .unwrap_or_default();

    let check_empty = args.empty;

    let output: Vec<Value> = records.into_iter().filter(|record| {
        if fields.is_empty() {
            // No fields specified: remove null records
            if record.is_null() {
                return false;
            }
            if check_empty {
                return !is_empty_value(record);
            }
            true
        } else {
            // Check specified fields
            for &field in &fields {
                if let Value::Object(map) = record {
                    match map.get(field) {
                        None | Some(Value::Null) => return false,
                        Some(v) if check_empty && is_empty_value(v) => return false,
                        _ => {}
                    }
                }
            }
            true
        }
    }).collect();

    Ok(output)
}

pub fn cmd_find(
    args: &args::FindArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("filter find: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let columns: Vec<&str> = args.columns.as_deref()
        .map(|s| zacor_package::parse_field_list(s))
        .unwrap_or_default();

    let re = if args.regex {
        let pattern = if args.ignore_case {
            format!("(?i){}", &args.term)
        } else {
            args.term.clone()
        };
        Some(regex::Regex::new(&pattern).map_err(|e| format!("filter find: invalid regex: {e}"))?)
    } else {
        None
    };

    let output: Vec<Value> = records.into_iter().filter(|record| {
        let matched = match record {
            Value::Object(map) => {
                let fields_to_search: Box<dyn Iterator<Item = (&String, &Value)>> = if columns.is_empty() {
                    Box::new(map.iter())
                } else {
                    Box::new(map.iter().filter(|(k, _)| columns.contains(&k.as_str())))
                };

                fields_to_search.into_iter().any(|(_, v)| {
                    if let Value::String(s) = v {
                        match &re {
                            Some(re) => re.is_match(s),
                            None => {
                                if args.ignore_case {
                                    s.to_lowercase().contains(&args.term.to_lowercase())
                                } else {
                                    s.contains(&args.term)
                                }
                            }
                        }
                    } else {
                        false
                    }
                })
            }
            _ => false,
        };

        if args.invert { !matched } else { matched }
    }).collect();

    Ok(output)
}

fn extract_key(record: &Value, fields: &[&str]) -> String {
    fields.iter()
        .map(|f| {
            match record {
                Value::Object(map) => map.get(*f)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                _ => "null".to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\0")
}

fn values_equal_ignore_case(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            if ma.len() != mb.len() {
                return false;
            }
            ma.iter().all(|(k, va)| {
                mb.get(k).map_or(false, |vb| values_equal_ignore_case(va, vb))
            })
        }
        (Value::String(sa), Value::String(sb)) => sa.to_lowercase() == sb.to_lowercase(),
        _ => a == b,
    }
}

fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(m) => m.is_empty(),
        _ => false,
    }
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
    fn skip_3_from_5() {
        let data = r#"[{"a":1},{"a":2},{"a":3},{"a":4},{"a":5}]"#;
        let args: args::SkipArgs = make_args(&[("count", json!(3))]);
        let result = cmd_skip(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["a"], 4);
    }

    #[test]
    fn skip_more_than_available() {
        let data = r#"[{"a":1},{"a":2}]"#;
        let args: args::SkipArgs = make_args(&[("count", json!(10))]);
        let result = cmd_skip(&args, json_input(data)).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn drop_2_from_5() {
        let data = r#"[{"a":1},{"a":2},{"a":3},{"a":4},{"a":5}]"#;
        let args: args::DropArgs = make_args(&[("count", json!(2))]);
        let result = cmd_drop(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[2]["a"], 3);
    }

    #[test]
    fn drop_default() {
        let data = r#"[{"a":1},{"a":2},{"a":3}]"#;
        let args: args::DropArgs = make_args(&[]);
        let result = cmd_drop(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn uniq_consecutive() {
        let data = r#"[{"a":1},{"a":1},{"a":2},{"a":2},{"a":1}]"#;
        let args: args::UniqArgs = make_args(&[]);
        let result = cmd_uniq(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["a"], 1);
        assert_eq!(result[1]["a"], 2);
        assert_eq!(result[2]["a"], 1);
    }

    #[test]
    fn uniq_with_count() {
        let data = r#"[{"a":1},{"a":1},{"a":2}]"#;
        let args: args::UniqArgs = make_args(&[("count", json!(true))]);
        let result = cmd_uniq(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["count"], 2);
        assert_eq!(result[1]["count"], 1);
    }

    #[test]
    fn uniq_by_field() {
        let data = r#"[{"name":"a","v":1},{"name":"a","v":2},{"name":"b","v":3}]"#;
        let args: args::UniqByArgs = make_args(&[("fields", json!("name"))]);
        let result = cmd_uniq_by(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["v"], 1);
        assert_eq!(result[1]["v"], 3);
    }

    #[test]
    fn uniq_by_keep_last() {
        let data = r#"[{"name":"a","v":1},{"name":"a","v":2},{"name":"b","v":3}]"#;
        let args: args::UniqByArgs = make_args(&[("fields", json!("name")), ("keep-last", json!(true))]);
        let result = cmd_uniq_by(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["v"], 2);
    }

    #[test]
    fn compact_null_records() {
        let data = r#"[{"a":1},null,{"a":3}]"#;
        let args: args::CompactArgs = make_args(&[]);
        let result = cmd_compact(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn compact_by_field() {
        let data = r#"[{"a":1,"b":null},{"a":2,"b":"x"}]"#;
        let args: args::CompactArgs = make_args(&[("fields", json!("b"))]);
        let result = cmd_compact(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["a"], 2);
    }

    #[test]
    fn compact_empty_flag() {
        let data = r#"[{"a":""},{"a":"x"},{"a":null}]"#;
        let args: args::CompactArgs = make_args(&[("fields", json!("a")), ("empty", json!(true))]);
        let result = cmd_compact(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["a"], "x");
    }

    #[test]
    fn find_string() {
        let data = r#"[{"name":"foobar"},{"name":"baz"}]"#;
        let args: args::FindArgs = make_args(&[("term", json!("foo"))]);
        let result = cmd_find(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "foobar");
    }

    #[test]
    fn find_regex() {
        let data = r#"[{"name":"main.rs"},{"name":"readme.md"}]"#;
        let args: args::FindArgs = make_args(&[("term", json!("\\.rs$")), ("regex", json!(true))]);
        let result = cmd_find(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn find_column_restriction() {
        let data = r#"[{"name":"foo","desc":"bar"},{"name":"baz","desc":"foo"}]"#;
        let args: args::FindArgs = make_args(&[("term", json!("foo")), ("columns", json!("name"))]);
        let result = cmd_find(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "foo");
    }

    #[test]
    fn find_invert() {
        let data = r#"[{"name":"foobar"},{"name":"baz"}]"#;
        let args: args::FindArgs = make_args(&[("term", json!("foo")), ("invert", json!(true))]);
        let result = cmd_find(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "baz");
    }

    #[test]
    fn find_ignore_case() {
        let data = r#"[{"name":"FooBar"},{"name":"baz"}]"#;
        let args: args::FindArgs = make_args(&[("term", json!("foo")), ("ignore-case", json!(true))]);
        let result = cmd_find(&args, json_input(data)).unwrap();
        assert_eq!(result.len(), 1);
    }
}
