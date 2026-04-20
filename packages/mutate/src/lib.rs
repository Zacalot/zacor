use serde_json::{json, Value};
use std::io::BufRead;

zacor_package::include_args!();

enum MutateMode {
    Insert,
    Update,
    Upsert,
}

fn mutate_records(
    field: &str,
    value: &str,
    expr: &str,
    mode: MutateMode,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("mutate: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let has_value = !value.is_empty();
    let has_expr = !expr.is_empty();

    if has_value && has_expr {
        return Err("mutate: provide either value or expr, not both".into());
    }
    if !has_value && !has_expr {
        return Err("mutate: one of value or expr is required".into());
    }

    let output: Vec<Value> = records.into_iter().map(|record| {
        if let Value::Object(mut map) = record.clone() {
            let exists = map.contains_key(field);

            let should_set = match mode {
                MutateMode::Insert => {
                    if exists {
                        // Insert errors if field exists — pass through unchanged
                        return record;
                    }
                    true
                }
                MutateMode::Update => {
                    if !exists {
                        return record;
                    }
                    true
                }
                MutateMode::Upsert => true,
            };

            if should_set {
                let new_val = if has_expr {
                    zr_expr::eval_value(expr, &record).unwrap_or(Value::Null)
                } else {
                    // Try to parse as JSON, fall back to string
                    serde_json::from_str(value).unwrap_or_else(|_| json!(value))
                };
                map.insert(field.to_string(), new_val);
            }

            Value::Object(map)
        } else {
            record
        }
    }).collect();

    Ok(output)
}

pub fn cmd_insert(
    args: &args::InsertArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    mutate_records(&args.field, args.value.as_deref().unwrap_or(""), args.expr.as_deref().unwrap_or(""), MutateMode::Insert, input)
}

pub fn cmd_update(
    args: &args::UpdateArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    mutate_records(&args.field, args.value.as_deref().unwrap_or(""), args.expr.as_deref().unwrap_or(""), MutateMode::Update, input)
}

pub fn cmd_upsert(
    args: &args::UpsertArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    mutate_records(&args.field, args.value.as_deref().unwrap_or(""), args.expr.as_deref().unwrap_or(""), MutateMode::Upsert, input)
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
    fn insert_literal() {
        let data = r#"[{"name":"a"}]"#;
        let args: args::InsertArgs = make_args(&[("field", json!("tag")), ("value", json!("done"))]);
        let result = cmd_insert(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["tag"], "done");
    }

    #[test]
    fn insert_expr() {
        let data = r#"[{"price":10,"qty":3}]"#;
        let args: args::InsertArgs = make_args(&[("field", json!("total")), ("expr", json!("price * qty"))]);
        let result = cmd_insert(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["total"], 30.0);
    }

    #[test]
    fn insert_existing_field_skips() {
        let data = r#"[{"tag":"old"}]"#;
        let args: args::InsertArgs = make_args(&[("field", json!("tag")), ("value", json!("new"))]);
        let result = cmd_insert(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["tag"], "old");
    }

    #[test]
    fn update_literal() {
        let data = r#"[{"status":"pending"}]"#;
        let args: args::UpdateArgs = make_args(&[("field", json!("status")), ("value", json!("complete"))]);
        let result = cmd_update(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["status"], "complete");
    }

    #[test]
    fn update_expr() {
        let data = r#"[{"name":"foo"}]"#;
        let args: args::UpdateArgs = make_args(&[("field", json!("name")), ("expr", json!("upper(name)"))]);
        let result = cmd_update(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "FOO");
    }

    #[test]
    fn update_missing_field_passthrough() {
        let data = r#"[{"a":1}]"#;
        let args: args::UpdateArgs = make_args(&[("field", json!("missing")), ("value", json!("x"))]);
        let result = cmd_update(&args, json_input(data)).unwrap();
        assert!(result[0].get("missing").is_none());
    }

    #[test]
    fn upsert_adds_when_missing() {
        let data = r#"[{"a":1}]"#;
        let args: args::UpsertArgs = make_args(&[("field", json!("tag")), ("value", json!("new"))]);
        let result = cmd_upsert(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["tag"], "new");
    }

    #[test]
    fn upsert_updates_when_present() {
        let data = r#"[{"tag":"old"}]"#;
        let args: args::UpsertArgs = make_args(&[("field", json!("tag")), ("value", json!("new"))]);
        let result = cmd_upsert(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["tag"], "new");
    }

    #[test]
    fn upsert_with_expr() {
        let data = r#"[{"price":10,"qty":3}]"#;
        let args: args::UpsertArgs = make_args(&[("field", json!("total")), ("expr", json!("price * qty"))]);
        let result = cmd_upsert(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["total"], 30.0);
    }

    #[test]
    fn both_value_and_expr_error() {
        let data = r#"[{"a":1}]"#;
        let args: args::InsertArgs = make_args(&[("field", json!("x")), ("value", json!("y")), ("expr", json!("a + 1"))]);
        assert!(cmd_insert(&args, json_input(data)).is_err());
    }

    #[test]
    fn neither_value_nor_expr_error() {
        let data = r#"[{"a":1}]"#;
        let args: args::InsertArgs = make_args(&[("field", json!("x"))]);
        assert!(cmd_insert(&args, json_input(data)).is_err());
    }
}
