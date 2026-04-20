use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::BufRead;

zacor_package::include_args!();

// ─── Aggregation commands ───────────────────────────────────────────

pub fn cmd_sum(args: &args::SumArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    aggregate(args.field.as_deref(), input, |vals| if vals.is_empty() { 0.0 } else { vals.iter().sum() })
}

pub fn cmd_avg(args: &args::AvgArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    aggregate_nullable(args.field.as_deref(), input, |vals| {
        if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
    })
}

pub fn cmd_min(args: &args::MinArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    aggregate_nullable(args.field.as_deref(), input, |vals| {
        vals.iter().cloned().reduce(f64::min)
    })
}

pub fn cmd_max(args: &args::MaxArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    aggregate_nullable(args.field.as_deref(), input, |vals| {
        vals.iter().cloned().reduce(f64::max)
    })
}

pub fn cmd_median(args: &args::MedianArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    aggregate_nullable(args.field.as_deref(), input, |vals| {
        if vals.is_empty() { return None; }
        let mut sorted = vals.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 0 {
            Some((sorted[mid - 1] + sorted[mid]) / 2.0)
        } else {
            Some(sorted[mid])
        }
    })
}

pub fn cmd_mode(args: &args::ModeArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    aggregate_nullable(args.field.as_deref(), input, |vals| {
        if vals.is_empty() { return None; }
        let mut counts: HashMap<u64, usize> = HashMap::new();
        for &v in vals {
            *counts.entry(v.to_bits()).or_insert(0) += 1;
        }
        counts.into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(bits, _)| f64::from_bits(bits))
    })
}

pub fn cmd_product(args: &args::ProductArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    aggregate(args.field.as_deref(), input, |vals| {
        if vals.is_empty() { 1.0 } else { vals.iter().product() }
    })
}

pub fn cmd_stddev(args: &args::StddevArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let sample = args.sample;
    aggregate_nullable(args.field.as_deref(), input, move |vals| {
        variance_impl(vals, sample).map(|v| v.sqrt())
    })
}

pub fn cmd_variance(args: &args::VarianceArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let sample = args.sample;
    aggregate_nullable(args.field.as_deref(), input, move |vals| {
        variance_impl(vals, sample)
    })
}

fn variance_impl(vals: &[f64], sample: bool) -> Option<f64> {
    let n = vals.len();
    if n == 0 { return None; }
    let denom = if sample { n - 1 } else { n };
    if denom == 0 { return None; }
    let mean = vals.iter().sum::<f64>() / n as f64;
    let sum_sq: f64 = vals.iter().map(|v| (v - mean).powi(2)).sum();
    Some(sum_sq / denom as f64)
}

pub fn cmd_count(_args: &args::CountArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("math count: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;
    Ok(vec![json!({"count": records.len()})])
}

// ─── Element-wise commands ──────────────────────────────────────────

pub fn cmd_round(args: &args::RoundArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let precision = args.precision.max(0) as u32;
    let factor = 10f64.powi(precision as i32);
    element_wise(&args.field, input, |v| {
        v.as_f64().map(|n| Value::from((n * factor).round() / factor))
    })
}

pub fn cmd_ceil(args: &args::CeilArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    element_wise(&args.field, input, |v| {
        v.as_f64().map(|n| Value::from(n.ceil()))
    })
}

pub fn cmd_floor(args: &args::FloorArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    element_wise(&args.field, input, |v| {
        v.as_f64().map(|n| Value::from(n.floor()))
    })
}

pub fn cmd_abs(args: &args::AbsArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    element_wise(&args.field, input, |v| {
        v.as_f64().map(|n| Value::from(n.abs()))
    })
}

// ─── Helpers ────────────────────────────────────────────────────────

fn aggregate(
    field: Option<&str>,
    input: Option<Box<dyn BufRead>>,
    func: impl Fn(&[f64]) -> f64,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("math: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    if let Some(field_name) = field {
        let vals = extract_numbers(&records, field_name);
        Ok(vec![json!({"value": func(&vals)})])
    } else {
        // Table mode: aggregate per numeric column
        let result = table_mode_aggregate(&records, |vals| Value::from(func(vals)));
        Ok(vec![result])
    }
}

fn aggregate_nullable(
    field: Option<&str>,
    input: Option<Box<dyn BufRead>>,
    func: impl Fn(&[f64]) -> Option<f64>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("math: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    if let Some(field_name) = field {
        let vals = extract_numbers(&records, field_name);
        let result = func(&vals).map(Value::from).unwrap_or(Value::Null);
        Ok(vec![json!({"value": result})])
    } else {
        let result = table_mode_aggregate(&records, |vals| {
            func(vals).map(Value::from).unwrap_or(Value::Null)
        });
        Ok(vec![result])
    }
}

fn table_mode_aggregate(records: &[Value], func: impl Fn(&[f64]) -> Value) -> Value {
    if records.is_empty() {
        return json!({});
    }

    // Get all numeric field names from first record
    let fields: Vec<String> = if let Value::Object(map) = &records[0] {
        map.iter()
            .filter(|(_, v)| v.as_f64().is_some())
            .map(|(k, _)| k.clone())
            .collect()
    } else {
        return json!({});
    };

    let mut result = Map::new();
    for field in &fields {
        let vals = extract_numbers(records, field);
        result.insert(field.clone(), func(&vals));
    }
    Value::Object(result)
}

fn extract_numbers(records: &[Value], field: &str) -> Vec<f64> {
    records.iter()
        .filter_map(|r| r.as_object()?.get(field)?.as_f64())
        .collect()
}

fn element_wise(
    field: &str,
    input: Option<Box<dyn BufRead>>,
    transform: impl Fn(&Value) -> Option<Value>,
) -> Result<Vec<Value>, String> {
    let reader = input.ok_or("math: requires piped input")?;
    let records = zacor_package::parse_records(reader)?;

    let output: Vec<Value> = records.into_iter().map(|record| {
        if let Value::Object(mut map) = record {
            if let Some(val) = map.get(field) {
                if let Some(new_val) = transform(val) {
                    map.insert(field.to_string(), new_val);
                }
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
    fn sum_field() {
        let data = r#"[{"price":10},{"price":20},{"price":30}]"#;
        let args: args::SumArgs = make_args(&[("field", json!("price"))]);
        let result = cmd_sum(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["value"], 60.0);
    }

    #[test]
    fn avg_field() {
        let data = r#"[{"v":10},{"v":20},{"v":30}]"#;
        let args: args::AvgArgs = make_args(&[("field", json!("v"))]);
        let result = cmd_avg(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["value"], 20.0);
    }

    #[test]
    fn min_max() {
        let data = r#"[{"v":5},{"v":1},{"v":9}]"#;
        let min_args: args::MinArgs = make_args(&[("field", json!("v"))]);
        let max_args: args::MaxArgs = make_args(&[("field", json!("v"))]);
        assert_eq!(cmd_min(&min_args, json_input(data)).unwrap()[0]["value"], 1.0);
        assert_eq!(cmd_max(&max_args, json_input(data)).unwrap()[0]["value"], 9.0);
    }

    #[test]
    fn median_odd() {
        let data = r#"[{"v":1},{"v":3},{"v":5}]"#;
        let args: args::MedianArgs = make_args(&[("field", json!("v"))]);
        let result = cmd_median(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["value"], 3.0);
    }

    #[test]
    fn median_even() {
        let data = r#"[{"v":1},{"v":2},{"v":3},{"v":4}]"#;
        let args: args::MedianArgs = make_args(&[("field", json!("v"))]);
        let result = cmd_median(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["value"], 2.5);
    }

    #[test]
    fn mode_value() {
        let data = r#"[{"v":1},{"v":2},{"v":2},{"v":3}]"#;
        let args: args::ModeArgs = make_args(&[("field", json!("v"))]);
        let result = cmd_mode(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["value"], 2.0);
    }

    #[test]
    fn count_records() {
        let data = r#"[{"a":1},{"a":2},{"a":3},{"a":4},{"a":5}]"#;
        let args: args::CountArgs = make_args(&[]);
        let result = cmd_count(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["count"], 5);
    }

    #[test]
    fn count_empty() {
        let args: args::CountArgs = make_args(&[]);
        let result = cmd_count(&args, json_input("[]")).unwrap();
        assert_eq!(result[0]["count"], 0);
    }

    #[test]
    fn round_with_precision() {
        let data = r#"[{"price":3.14159}]"#;
        let args: args::RoundArgs = make_args(&[("field", json!("price")), ("precision", json!(2))]);
        let result = cmd_round(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["price"], 3.14);
    }

    #[test]
    fn ceil_value() {
        let data = r#"[{"v":2.1}]"#;
        let args: args::CeilArgs = make_args(&[("field", json!("v"))]);
        let result = cmd_ceil(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["v"], 3.0);
    }

    #[test]
    fn floor_value() {
        let data = r#"[{"v":2.9}]"#;
        let args: args::FloorArgs = make_args(&[("field", json!("v"))]);
        let result = cmd_floor(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["v"], 2.0);
    }

    #[test]
    fn abs_value() {
        let data = r#"[{"v":-5}]"#;
        let args: args::AbsArgs = make_args(&[("field", json!("v"))]);
        let result = cmd_abs(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["v"], 5.0);
    }

    #[test]
    fn sum_table_mode() {
        let data = r#"[{"a":1,"b":2},{"a":3,"b":4}]"#;
        let args: args::SumArgs = make_args(&[]);
        let result = cmd_sum(&args, json_input(data)).unwrap();
        assert_eq!(result[0]["a"], 4.0);
        assert_eq!(result[0]["b"], 6.0);
    }

    #[test]
    fn avg_empty() {
        let args: args::AvgArgs = make_args(&[("field", json!("v"))]);
        let result = cmd_avg(&args, json_input("[]")).unwrap();
        assert!(result[0]["value"].is_null());
    }

    #[test]
    fn sum_empty() {
        let args: args::SumArgs = make_args(&[("field", json!("v"))]);
        let result = cmd_sum(&args, json_input("[]")).unwrap();
        assert_eq!(result[0]["value"], 0.0);
    }

    #[test]
    fn stddev_population() {
        let data = r#"[{"v":2},{"v":4},{"v":4},{"v":4},{"v":5},{"v":5},{"v":7},{"v":9}]"#;
        let args: args::StddevArgs = make_args(&[("field", json!("v"))]);
        let result = cmd_stddev(&args, json_input(data)).unwrap();
        let stddev = result[0]["value"].as_f64().unwrap();
        assert!((stddev - 2.0).abs() < 0.01);
    }

    #[test]
    fn variance_sample() {
        let data = r#"[{"v":2},{"v":4},{"v":4},{"v":4},{"v":5},{"v":5},{"v":7},{"v":9}]"#;
        let args: args::VarianceArgs = make_args(&[("field", json!("v")), ("sample", json!(true))]);
        let result = cmd_variance(&args, json_input(data)).unwrap();
        let var = result[0]["value"].as_f64().unwrap();
        assert!((var - 4.571).abs() < 0.01);
    }
}
