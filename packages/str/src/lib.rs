use regex::Regex;
use serde_json::{json, Value};
use std::io::BufRead;

zacor_package::include_args!();

fn resolve_transform_input(
    value: Option<&str>,
    input: Option<Box<dyn BufRead>>,
) -> Result<(String, Option<Box<dyn BufRead>>), String> {
    if input.is_some() {
        return Ok(("value".to_string(), input));
    }

    let value = value.ok_or("str: requires piped input or an inline value")?;

    Ok((
        "value".to_string(),
        Some(Box::new(std::io::Cursor::new(
            format!("{}\n", json!({"value": value})).into_bytes(),
        ))),
    ))
}

fn resolve_fields(fields: Option<&str>) -> String {
    fields.unwrap_or("value").to_string()
}

fn transform_fields(
    value: Option<&str>,
    fields: Option<&str>,
    input: Option<Box<dyn BufRead>>,
    transform: impl Fn(&Value) -> Value,
) -> Result<Vec<Value>, String> {
    let (default_fields, input) = resolve_transform_input(value, input)?;
    let fields = fields
        .map(|value| value.to_string())
        .unwrap_or(default_fields);
    let reader = input.ok_or("str: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;
    let field_list = zacor_package::parse_field_list(&fields);

    let output: Vec<Value> = records.into_iter().map(|record| {
        if let Value::Object(mut map) = record {
            for &f in &field_list {
                if let Some(val) = map.get(f) {
                    let new_val = transform(val);
                    map.insert(f.to_string(), new_val);
                }
            }
            Value::Object(map)
        } else {
            record
        }
    }).collect();

    Ok(output)
}

pub fn cmd_trim(args: &args::TrimArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let char_to_trim = args.char.clone();
    let left = args.left;
    let right = args.right;

    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v {
            let result = if let Some(ref ch) = char_to_trim {
                let c: Vec<char> = ch.chars().collect();
                let pat: &[char] = &c;
                if left && !right {
                    s.trim_start_matches(pat).to_string()
                } else if right && !left {
                    s.trim_end_matches(pat).to_string()
                } else {
                    s.trim_matches(pat).to_string()
                }
            } else if left && !right {
                s.trim_start().to_string()
            } else if right && !left {
                s.trim_end().to_string()
            } else {
                s.trim().to_string()
            };
            Value::String(result)
        } else {
            v.clone()
        }
    })
}

pub fn cmd_replace(args: &args::ReplaceArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let find = args.find.clone();
    let replacement = args.replacement.clone();
    let all = args.all;
    let use_regex = args.regex;

    let re = if use_regex {
        Some(Regex::new(&find).map_err(|e| format!("str replace: invalid regex: {e}"))?)
    } else {
        None
    };

    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v {
            let result = if let Some(ref re) = re {
                if all {
                    re.replace_all(s, replacement.as_str()).to_string()
                } else {
                    re.replace(s, replacement.as_str()).to_string()
                }
            } else if all {
                s.replace(&find, &replacement)
            } else {
                s.replacen(&find, &replacement, 1)
            };
            Value::String(result)
        } else {
            v.clone()
        }
    })
}

pub fn cmd_upcase(args: &args::UpcaseArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v { Value::String(s.to_uppercase()) } else { v.clone() }
    })
}

pub fn cmd_downcase(args: &args::DowncaseArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v { Value::String(s.to_lowercase()) } else { v.clone() }
    })
}

pub fn cmd_capitalize(args: &args::CapitalizeArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v {
            let mut chars = s.chars();
            let result = match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            };
            Value::String(result)
        } else {
            v.clone()
        }
    })
}

pub fn cmd_reverse(args: &args::ReverseArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v { Value::String(s.chars().rev().collect()) } else { v.clone() }
    })
}

pub fn cmd_substring(args: &args::SubstringArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let range = args.range.clone();
    let parts: Vec<&str> = range.split("..").collect();
    let start: usize = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let end: Option<usize> = parts.get(1).and_then(|s| s.parse().ok());

    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v {
            let chars: Vec<char> = s.chars().collect();
            let end_idx = end.unwrap_or(chars.len()).min(chars.len());
            let start_idx = start.min(end_idx);
            let result: String = chars[start_idx..end_idx].iter().collect();
            Value::String(result)
        } else {
            v.clone()
        }
    })
}

pub fn cmd_contains(args: &args::ContainsArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let term = args.term.clone();
    let ignore_case = args.ignore_case;

    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v {
            let result = if ignore_case {
                s.to_lowercase().contains(&term.to_lowercase())
            } else {
                s.contains(&term)
            };
            Value::Bool(result)
        } else {
            v.clone()
        }
    })
}

pub fn cmd_starts_with(args: &args::StartsWithArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let term = args.term.clone();
    let ignore_case = args.ignore_case;

    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v {
            let result = if ignore_case {
                s.to_lowercase().starts_with(&term.to_lowercase())
            } else {
                s.starts_with(&term)
            };
            Value::Bool(result)
        } else {
            v.clone()
        }
    })
}

pub fn cmd_ends_with(args: &args::EndsWithArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let term = args.term.clone();
    let ignore_case = args.ignore_case;

    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v {
            let result = if ignore_case {
                s.to_lowercase().ends_with(&term.to_lowercase())
            } else {
                s.ends_with(&term)
            };
            Value::Bool(result)
        } else {
            v.clone()
        }
    })
}

pub fn cmd_length(args: &args::LengthArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v { json!(s.len()) } else { v.clone() }
    })
}

pub fn cmd_index_of(args: &args::IndexOfArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let term = args.term.clone();
    let from_end = args.end;

    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v {
            let idx = if from_end {
                s.rfind(&term).map(|i| i as i64).unwrap_or(-1)
            } else {
                s.find(&term).map(|i| i as i64).unwrap_or(-1)
            };
            json!(idx)
        } else {
            v.clone()
        }
    })
}

pub fn cmd_split(args: &args::SplitArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let separator = args.separator.clone();
    let use_regex = args.regex;

    let re = if use_regex {
        Some(Regex::new(&separator).map_err(|e| format!("str split: invalid regex: {e}"))?)
    } else {
        None
    };

    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::String(s) = v {
            let parts: Vec<Value> = if let Some(ref re) = re {
                re.split(s).map(|p| json!(p)).collect()
            } else {
                s.split(&separator).map(|p| json!(p)).collect()
            };
            Value::Array(parts)
        } else {
            v.clone()
        }
    })
}

pub fn cmd_join(args: &args::JoinArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let separator = args.separator.clone();

    transform_fields(args.value.as_deref(), args.fields.as_deref(), input, |v| {
        if let Value::Array(arr) = v {
            let parts: Vec<String> = arr.iter().map(|item| {
                match item {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                }
            }).collect();
            Value::String(parts.join(&separator))
        } else {
            v.clone()
        }
    })
}

pub fn cmd_parse(args: &args::ParseArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("str parse: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;
    let fields = resolve_fields(args.fields.as_deref());
    let field_list = zacor_package::parse_field_list(&fields);

    let re = if args.regex {
        Regex::new(&args.pattern).map_err(|e| format!("str parse: invalid regex: {e}"))?
    } else {
        // Convert simple pattern like "{ip} - {user}" to regex with named groups
        let regex_pattern = convert_pattern_to_regex(&args.pattern);
        Regex::new(&regex_pattern).map_err(|e| format!("str parse: invalid pattern: {e}"))?
    };

    let output: Vec<Value> = records.into_iter().map(|record| {
        if let Value::Object(mut map) = record {
            for &f in &field_list {
                let s = match map.get(f) {
                    Some(Value::String(s)) => s.clone(),
                    _ => continue,
                };
                if let Some(caps) = re.captures(&s) {
                    for name in re.capture_names().flatten() {
                        if let Some(m) = caps.name(name) {
                            map.insert(name.to_string(), json!(m.as_str()));
                        }
                    }
                }
            }
            Value::Object(map)
        } else {
            record
        }
    }).collect();

    Ok(output)
}

fn convert_pattern_to_regex(pattern: &str) -> String {
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut name = String::new();
            while let Some(&c) = chars.peek() {
                if c == '}' {
                    chars.next();
                    break;
                }
                name.push(c);
                chars.next();
            }
            result.push_str(&format!("(?P<{name}>\\S+)"));
        } else {
            // Escape regex special chars
            if ".*+?^${}()|[]\\".contains(ch) {
                result.push('\\');
            }
            result.push(ch);
        }
    }

    result
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
    fn inline_scalar_uses_value_field() {
        let args: args::CapitalizeArgs = make_args(&[("value", json!("hello world"))]);
        let result = cmd_capitalize(&args, None).unwrap();
        assert_eq!(result, vec![json!({"value": "Hello world"})]);
    }

    #[test]
    fn piped_input_defaults_to_value_field() {
        let data = r#"[{"value":"hello"}]"#;
        let args: args::CapitalizeArgs = make_args(&[]);
        let result = cmd_capitalize(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["value"], "Hello");
    }

    #[test]
    fn explicit_fields_override_default_value_field() {
        let data = r#"[{"name":"hello"}]"#;
        let args: args::CapitalizeArgs = make_args(&[("fields", json!("name"))]);
        let result = cmd_capitalize(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "Hello");
    }

    #[test]
    fn trim_whitespace() {
        let data = r#"[{"name":"  hello  "}]"#;
        let args: args::TrimArgs = make_args(&[("fields", json!("name"))]);
        let result = cmd_trim(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "hello");
    }

    #[test]
    fn trim_char() {
        let data = r#"[{"name":"===hello==="}]"#;
        let args: args::TrimArgs = make_args(&[("fields", json!("name")), ("char", json!("="))]);
        let result = cmd_trim(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "hello");
    }

    #[test]
    fn trim_left() {
        let data = r#"[{"name":"  hello  "}]"#;
        let args: args::TrimArgs = make_args(&[("fields", json!("name")), ("left", json!(true))]);
        let result = cmd_trim(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "hello  ");
    }

    #[test]
    fn replace_first() {
        let data = r#"[{"path":"src/src/main.rs"}]"#;
        let args: args::ReplaceArgs = make_args(&[("fields", json!("path")), ("find", json!("src")), ("replacement", json!("dist"))]);
        let result = cmd_replace(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["path"], "dist/src/main.rs");
    }

    #[test]
    fn replace_all() {
        let data = r#"[{"path":"src/src/main.rs"}]"#;
        let args: args::ReplaceArgs = make_args(&[("fields", json!("path")), ("find", json!("src")), ("replacement", json!("dist")), ("all", json!(true))]);
        let result = cmd_replace(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["path"], "dist/dist/main.rs");
    }

    #[test]
    fn upcase() {
        let data = r#"[{"name":"hello"}]"#;
        let args: args::UpcaseArgs = make_args(&[("fields", json!("name"))]);
        let result = cmd_upcase(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "HELLO");
    }

    #[test]
    fn downcase() {
        let data = r#"[{"name":"HELLO"}]"#;
        let args: args::DowncaseArgs = make_args(&[("fields", json!("name"))]);
        let result = cmd_downcase(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "hello");
    }

    #[test]
    fn capitalize() {
        let data = r#"[{"name":"hello world"}]"#;
        let args: args::CapitalizeArgs = make_args(&[("fields", json!("name"))]);
        let result = cmd_capitalize(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "Hello world");
    }

    #[test]
    fn reverse_string() {
        let data = r#"[{"name":"hello"}]"#;
        let args: args::ReverseArgs = make_args(&[("fields", json!("name"))]);
        let result = cmd_reverse(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "olleh");
    }

    #[test]
    fn substring_range() {
        let data = r#"[{"name":"hello world"}]"#;
        let args: args::SubstringArgs = make_args(&[("fields", json!("name")), ("range", json!("0..5"))]);
        let result = cmd_substring(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "hello");
    }

    #[test]
    fn contains_test() {
        let data = r#"[{"name":"foobar"},{"name":"baz"}]"#;
        let args: args::ContainsArgs = make_args(&[("fields", json!("name")), ("term", json!("foo"))]);
        let result = cmd_contains(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], true);
        assert_eq!(result[1]["name"], false);
    }

    #[test]
    fn starts_with_test() {
        let data = r#"[{"name":"foobar"}]"#;
        let args: args::StartsWithArgs = make_args(&[("fields", json!("name")), ("term", json!("foo"))]);
        let result = cmd_starts_with(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], true);
    }

    #[test]
    fn ends_with_test() {
        let data = r#"[{"name":"main.rs"}]"#;
        let args: args::EndsWithArgs = make_args(&[("fields", json!("name")), ("term", json!(".rs"))]);
        let result = cmd_ends_with(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], true);
    }

    #[test]
    fn length_test() {
        let data = r#"[{"name":"hello"}]"#;
        let args: args::LengthArgs = make_args(&[("fields", json!("name"))]);
        let result = cmd_length(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], 5);
    }

    #[test]
    fn index_of_test() {
        let data = r#"[{"name":"hello world"}]"#;
        let args: args::IndexOfArgs = make_args(&[("fields", json!("name")), ("term", json!("world"))]);
        let result = cmd_index_of(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], 6);
    }

    #[test]
    fn index_of_not_found() {
        let data = r#"[{"name":"hello"}]"#;
        let args: args::IndexOfArgs = make_args(&[("fields", json!("name")), ("term", json!("xyz"))]);
        let result = cmd_index_of(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], -1);
    }

    #[test]
    fn split_test() {
        let data = r#"[{"name":"a,b,c"}]"#;
        let args: args::SplitArgs = make_args(&[("fields", json!("name")), ("separator", json!(","))]);
        let result = cmd_split(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], json!(["a", "b", "c"]));
    }

    #[test]
    fn join_test() {
        let data = r#"[{"name":["a","b","c"]}]"#;
        let args: args::JoinArgs = make_args(&[("fields", json!("name")), ("separator", json!("-"))]);
        let result = cmd_join(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], "a-b-c");
    }

    #[test]
    fn parse_simple_pattern() {
        let data = r#"[{"log":"192.168.1.1 - admin"}]"#;
        let args: args::ParseArgs = make_args(&[("fields", json!("log")), ("pattern", json!("{ip} - {user}"))]);
        let result = cmd_parse(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["ip"], "192.168.1.1");
        assert_eq!(result[0]["user"], "admin");
    }

    #[test]
    fn parse_regex_pattern() {
        let data = r#"[{"log":"192.168.1.1 - admin"}]"#;
        let args: args::ParseArgs = make_args(&[("fields", json!("log")), ("pattern", json!("(?P<ip>\\S+) - (?P<user>\\S+)")), ("regex", json!(true))]);
        let result = cmd_parse(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["ip"], "192.168.1.1");
        assert_eq!(result[0]["user"], "admin");
    }

    #[test]
    fn multiple_fields() {
        let data = r#"[{"first":"hello","last":"world"}]"#;
        let args: args::UpcaseArgs = make_args(&[("fields", json!("first last"))]);
        let result = cmd_upcase(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["first"], "HELLO");
        assert_eq!(result[0]["last"], "WORLD");
    }

    #[test]
    fn missing_field_skipped() {
        let data = r#"[{"a":"hello"}]"#;
        let args: args::UpcaseArgs = make_args(&[("fields", json!("missing"))]);
        let result = cmd_upcase(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["a"], "hello");
    }

    #[test]
    fn ignore_case_contains() {
        let data = r#"[{"name":"FooBar"}]"#;
        let args: args::ContainsArgs = make_args(&[("fields", json!("name")), ("term", json!("foo")), ("ignore-case", json!(true))]);
        let result = cmd_contains(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["name"], true);
    }
}
