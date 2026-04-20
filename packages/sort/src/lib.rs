use serde_json::Value;
use std::io::BufRead;

zacor_package::include_args!();

pub fn cmd_by(
    args: &args::ByArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("sort by: requires piped input")?;
    let mut records = zacor_package::parse_records(reader)?;

    let fields = zacor_package::parse_field_list(&args.fields);

    if fields.is_empty() {
        return Err("sort by: at least one field is required".into());
    }

    let reverse = args.reverse;
    let natural = args.natural;
    let ignore_case = args.ignore_case;

    records.sort_by(|a, b| {
        for &field in &fields {
            let va = resolve_field(a, field);
            let vb = resolve_field(b, field);
            let ord = compare_values(&va, &vb, natural, ignore_case);
            if ord != std::cmp::Ordering::Equal {
                return if reverse { ord.reverse() } else { ord };
            }
        }
        std::cmp::Ordering::Equal
    });

    Ok(records)
}

pub fn cmd_reverse(
    _args: &args::ReverseArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("sort reverse: requires piped input")?;
    let mut records = zacor_package::parse_records(reader)?;
    records.reverse();
    Ok(records)
}

fn resolve_field<'a>(record: &'a Value, path: &str) -> &'a Value {
    let mut current = record;
    for part in path.split('.') {
        match current {
            Value::Object(map) => {
                current = match map.get(part) {
                    Some(v) => v,
                    None => return &Value::Null,
                };
            }
            _ => return &Value::Null,
        }
    }
    current
}

fn compare_values(a: &Value, b: &Value, natural: bool, ignore_case: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    // Nulls sort last
    match (a.is_null(), b.is_null()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    // Try numeric comparison
    if let (Some(na), Some(nb)) = (a.as_f64(), b.as_f64()) {
        return na.partial_cmp(&nb).unwrap_or(Ordering::Equal);
    }

    // String comparison
    let sa = value_to_string(a);
    let sb = value_to_string(b);

    if natural {
        natural_cmp(&sa, &sb, ignore_case)
    } else if ignore_case {
        sa.to_lowercase().cmp(&sb.to_lowercase())
    } else {
        sa.cmp(&sb)
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn natural_cmp(a: &str, b: &str, ignore_case: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let a_chars: Vec<char> = if ignore_case {
        a.to_lowercase().chars().collect()
    } else {
        a.chars().collect()
    };
    let b_chars: Vec<char> = if ignore_case {
        b.to_lowercase().chars().collect()
    } else {
        b.chars().collect()
    };

    let mut ai = 0;
    let mut bi = 0;

    while ai < a_chars.len() && bi < b_chars.len() {
        let ac = a_chars[ai];
        let bc = b_chars[bi];

        if ac.is_ascii_digit() && bc.is_ascii_digit() {
            // Compare numeric segments
            let mut an = String::new();
            while ai < a_chars.len() && a_chars[ai].is_ascii_digit() {
                an.push(a_chars[ai]);
                ai += 1;
            }
            let mut bn = String::new();
            while bi < b_chars.len() && b_chars[bi].is_ascii_digit() {
                bn.push(b_chars[bi]);
                bi += 1;
            }
            let na: u64 = an.parse().unwrap_or(0);
            let nb: u64 = bn.parse().unwrap_or(0);
            match na.cmp(&nb) {
                Ordering::Equal => continue,
                other => return other,
            }
        }

        match ac.cmp(&bc) {
            Ordering::Equal => {
                ai += 1;
                bi += 1;
            }
            other => return other,
        }
    }

    a_chars.len().cmp(&b_chars.len())
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
    fn sort_single_field() {
        let data = r#"[{"name":"c"},{"name":"a"},{"name":"b"}]"#;
        let args: args::ByArgs = make_args(&[("fields", json!("name"))]);
        let result = cmd_by(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "a");
        assert_eq!(result[1]["name"], "b");
        assert_eq!(result[2]["name"], "c");
    }

    #[test]
    fn sort_multi_field() {
        let data = r#"[{"type":"b","name":"z"},{"type":"a","name":"y"},{"type":"a","name":"x"}]"#;
        let args: args::ByArgs = make_args(&[("fields", json!("type name"))]);
        let result = cmd_by(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "x");
        assert_eq!(result[1]["name"], "y");
        assert_eq!(result[2]["name"], "z");
    }

    #[test]
    fn sort_numeric() {
        let data = r#"[{"v":10},{"v":2},{"v":1}]"#;
        let args: args::ByArgs = make_args(&[("fields", json!("v"))]);
        let result = cmd_by(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["v"], 1);
        assert_eq!(result[1]["v"], 2);
        assert_eq!(result[2]["v"], 10);
    }

    #[test]
    fn sort_reverse() {
        let data = r#"[{"v":1},{"v":2},{"v":3}]"#;
        let args: args::ByArgs = make_args(&[("fields", json!("v")), ("reverse", json!(true))]);
        let result = cmd_by(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["v"], 3);
        assert_eq!(result[2]["v"], 1);
    }

    #[test]
    fn sort_natural() {
        let data = r#"[{"f":"file10"},{"f":"file2"},{"f":"file1"}]"#;
        let args: args::ByArgs = make_args(&[("fields", json!("f")), ("natural", json!(true))]);
        let result = cmd_by(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["f"], "file1");
        assert_eq!(result[1]["f"], "file2");
        assert_eq!(result[2]["f"], "file10");
    }

    #[test]
    fn sort_ignore_case() {
        let data = r#"[{"n":"Banana"},{"n":"apple"},{"n":"Cherry"}]"#;
        let args: args::ByArgs = make_args(&[("fields", json!("n")), ("ignore-case", json!(true))]);
        let result = cmd_by(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["n"], "apple");
        assert_eq!(result[1]["n"], "Banana");
        assert_eq!(result[2]["n"], "Cherry");
    }

    #[test]
    fn sort_missing_fields_last() {
        let data = r#"[{"v":2},{"name":"foo"},{"v":1}]"#;
        let args: args::ByArgs = make_args(&[("fields", json!("v"))]);
        let result = cmd_by(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["v"], 1);
        assert_eq!(result[1]["v"], 2);
        // Record without 'v' should be last
        assert_eq!(result[2]["name"], "foo");
    }

    #[test]
    fn sort_nested_path() {
        let data = r#"[{"info":{"size":30}},{"info":{"size":10}},{"info":{"size":20}}]"#;
        let args: args::ByArgs = make_args(&[("fields", json!("info.size"))]);
        let result = cmd_by(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["info"]["size"], 10);
        assert_eq!(result[2]["info"]["size"], 30);
    }

    #[test]
    fn reverse_order() {
        let data = r#"[{"a":1},{"a":2},{"a":3}]"#;
        let args: args::ReverseArgs = make_args(&[]);
        let result = cmd_reverse(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["a"], 3);
        assert_eq!(result[1]["a"], 2);
        assert_eq!(result[2]["a"], 1);
    }

    #[test]
    fn reverse_empty() {
        let args: args::ReverseArgs = make_args(&[]);
        let result = cmd_reverse(&args, json_input("[]")).unwrap();
        assert!(result.is_empty());
    }
}
