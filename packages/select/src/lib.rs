use serde_json::{Map, Value};
use std::io::BufRead;

zacor_package::include_args!();

pub fn cmd_default(
    args: &args::DefaultArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("select: requires piped input")?;
    select(&args.fields, reader)
}

pub fn cmd_reject(
    args: &args::RejectArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("select reject: requires piped input")?;
    reject(&args.fields, reader)
}

pub fn select(
    fields_str: &str,
    input: Box<dyn BufRead>,
) -> Result<Vec<Value>, String> {
    let fields = zacor_package::parse_field_list(fields_str);

    if fields.is_empty() {
        return Err("select: at least one field name is required".into());
    }

    let records = zacor_package::parse_records(input)?;
    let single = fields.len() == 1;

    let mut output = Vec::new();
    for record in &records {
        if let Value::Object(map) = record {
            if single {
                let val = map.get(fields[0]).cloned().unwrap_or(Value::Null);
                let mut wrapped = Map::new();
                wrapped.insert("value".to_string(), val);
                output.push(Value::Object(wrapped));
            } else {
                let mut projected = Map::new();
                for &f in &fields {
                    let val = map.get(f).cloned().unwrap_or(Value::Null);
                    projected.insert(f.to_string(), val);
                }
                output.push(Value::Object(projected));
            }
        }
    }

    Ok(output)
}

pub fn reject(
    fields_str: &str,
    input: Box<dyn BufRead>,
) -> Result<Vec<Value>, String> {
    let fields = zacor_package::parse_field_list(fields_str);

    if fields.is_empty() {
        return Err("select reject: at least one field name is required".into());
    }

    let records = zacor_package::parse_records(input)?;

    let output: Vec<Value> = records.into_iter().map(|record| {
        if let Value::Object(mut map) = record {
            for &f in &fields {
                map.remove(f);
            }
            Value::Object(map)
        } else {
            record
        }
    }).collect();

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn input(s: &str) -> Box<dyn BufRead> {
        Box::new(Cursor::new(s.to_string().into_bytes()))
    }

    #[test]
    fn single_field_projection() {
        let data = r#"[{"name":"foo","kind":"change"},{"name":"bar","kind":"change"}]"#;
        let result = select("name", input(data)).unwrap();
        assert_eq!(result[0].as_object().unwrap().get("value").unwrap(), "foo");
        assert_eq!(result[1].as_object().unwrap().get("value").unwrap(), "bar");
    }

    #[test]
    fn multi_field_projection() {
        let data = r#"[{"name":"foo","kind":"change","detail":"tasking"}]"#;
        let result = select("name detail", input(data)).unwrap();
        assert_eq!(result.len(), 1);
        let obj = result[0].as_object().unwrap();
        assert_eq!(obj.get("name").unwrap(), "foo");
        assert_eq!(obj.get("detail").unwrap(), "tasking");
        assert!(!obj.contains_key("kind"));
    }

    #[test]
    fn missing_field_returns_null() {
        let data = r#"[{"name":"foo"}]"#;
        let result = select("name kind", input(data)).unwrap();
        let obj = result[0].as_object().unwrap();
        assert_eq!(obj.get("name").unwrap(), "foo");
        assert!(obj.get("kind").unwrap().is_null());
    }

    #[test]
    fn array_input() {
        let data = r#"[{"a":1},{"a":2}]"#;
        let result = select("a", input(data)).unwrap();
        assert_eq!(result[0].as_object().unwrap().get("value").unwrap(), 1);
        assert_eq!(result[1].as_object().unwrap().get("value").unwrap(), 2);
    }

    #[test]
    fn jsonl_input() {
        let data = "{\"a\":1}\n{\"a\":2}";
        let result = select("a", input(data)).unwrap();
        assert_eq!(result[0].as_object().unwrap().get("value").unwrap(), 1);
        assert_eq!(result[1].as_object().unwrap().get("value").unwrap(), 2);
    }

    #[test]
    fn comma_separated_fields() {
        let data = r#"[{"name":"foo","detail":"bar","extra":"baz"}]"#;
        let result = select("name,detail", input(data)).unwrap();
        let obj = result[0].as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert_eq!(obj.get("name").unwrap(), "foo");
        assert_eq!(obj.get("detail").unwrap(), "bar");
    }

    #[test]
    fn empty_fields_error() {
        let data = r#"[{"a":1}]"#;
        assert!(select("", input(data)).is_err());
    }

    #[test]
    fn from_args_string_field() {
        use zacor_package::FromArgs;
        let map: std::collections::BTreeMap<String, _> = [("fields".into(), serde_json::json!("name"))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.fields, "name");
    }

    // ─── Reject tests ───────────────────────────────────────────────

    #[test]
    fn reject_single_field() {
        let data = r#"[{"name":"a","size":1,"kind":"f"}]"#;
        let result = reject("kind", input(data)).unwrap();
        let obj = result[0].as_object().unwrap();
        assert_eq!(obj.get("name").unwrap(), "a");
        assert_eq!(obj.get("size").unwrap(), 1);
        assert!(!obj.contains_key("kind"));
    }

    #[test]
    fn reject_multiple_fields() {
        let data = r#"[{"name":"a","size":1,"kind":"f"}]"#;
        let result = reject("kind size", input(data)).unwrap();
        let obj = result[0].as_object().unwrap();
        assert_eq!(obj.len(), 1);
        assert_eq!(obj.get("name").unwrap(), "a");
    }

    #[test]
    fn reject_missing_field() {
        let data = r#"[{"name":"a"}]"#;
        let result = reject("missing", input(data)).unwrap();
        assert_eq!(result[0]["name"], "a");
    }

    #[test]
    fn reject_all_fields() {
        let data = r#"[{"a":1,"b":2}]"#;
        let result = reject("a b", input(data)).unwrap();
        assert_eq!(result[0].as_object().unwrap().len(), 0);
    }
}
