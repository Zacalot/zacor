use std::io::BufRead;

use rand::seq::SliceRandom;
use serde_json::{Value, json};

use crate::args::{PickArgs, ShuffleArgs};
use crate::make_rng;

pub fn cmd_pick(args: &PickArgs, input: Option<Box<dyn BufRead>>) -> Result<Vec<Value>, String> {
    let count = args.count.unwrap_or(1) as usize;
    let replace = args.replace;
    let mut rng = make_rng(args.seed);

    let items: Vec<Value> = if let Some(ref values_str) = args.values {
        values_str
            .split(',')
            .map(|s| json!({"value": s.trim()}))
            .collect()
    } else if let Some(ref file_path) = args.file {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| format!("rand pick: cannot read file '{file_path}': {e}"))?;
        content
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| json!({"value": l}))
            .collect()
    } else if let Some(reader) = input {
        parse_input_records(reader)?
    } else {
        return Err("rand pick: requires values=, file=, or piped input".to_string());
    };

    if items.is_empty() {
        return Ok(Vec::new());
    }

    if replace {
        let mut results = Vec::with_capacity(count);
        for _ in 0..count {
            results.push(items.choose(&mut rng).unwrap().clone());
        }
        Ok(results)
    } else {
        let mut pool = items;
        pool.shuffle(&mut rng);
        pool.truncate(count);
        Ok(pool)
    }
}

pub fn cmd_shuffle(
    args: &ShuffleArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let mut rng = make_rng(args.seed);

    let reader = input.ok_or_else(|| "rand shuffle: requires piped input".to_string())?;
    let mut records = parse_input_records(reader)?;
    records.shuffle(&mut rng);
    Ok(records)
}

fn parse_input_records(reader: Box<dyn BufRead>) -> Result<Vec<Value>, String> {
    let mut records = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("rand: read error: {e}"))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: Value =
            serde_json::from_str(trimmed).map_err(|e| format!("rand: invalid JSON: {e}"))?;
        records.push(val);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use zacor_package::FromArgs;

    fn make_args<T: FromArgs>(pairs: &[(&str, Value)]) -> T {
        let map: BTreeMap<String, Value> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        T::from_args(&map).unwrap()
    }

    fn json_input(lines: &[&str]) -> Option<Box<dyn BufRead>> {
        let data = lines.join("\n");
        Some(Box::new(std::io::Cursor::new(data.into_bytes())))
    }

    #[test]
    fn pick_from_values() {
        let args: PickArgs = make_args(&[
            ("values", json!("red,green,blue")),
            ("count", json!(2)),
            ("seed", json!(42)),
        ]);
        let result = cmd_pick(&args, None).unwrap();
        assert_eq!(result.len(), 2);
        for r in &result {
            let v = r["value"].as_str().unwrap();
            assert!(["red", "green", "blue"].contains(&v));
        }
    }

    #[test]
    fn pick_from_input() {
        let input = json_input(&[
            r#"{"name":"alice","age":30}"#,
            r#"{"name":"bob","age":25}"#,
            r#"{"name":"carol","age":35}"#,
        ]);
        let args: PickArgs = make_args(&[("count", json!(2)), ("seed", json!(42))]);
        let result = cmd_pick(&args, input).unwrap();
        assert_eq!(result.len(), 2);
        for r in &result {
            assert!(r["name"].as_str().is_some());
        }
    }

    #[test]
    fn pick_count_exceeds_source() {
        let args: PickArgs = make_args(&[
            ("values", json!("a,b,c")),
            ("count", json!(10)),
            ("seed", json!(42)),
        ]);
        let result = cmd_pick(&args, None).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn pick_with_replacement() {
        let args: PickArgs = make_args(&[
            ("values", json!("heads,tails")),
            ("count", json!(10)),
            ("replace", json!(true)),
            ("seed", json!(42)),
        ]);
        let result = cmd_pick(&args, None).unwrap();
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn pick_seed_determinism() {
        let a: PickArgs = make_args(&[
            ("values", json!("a,b,c,d,e")),
            ("count", json!(3)),
            ("seed", json!(42)),
        ]);
        let b: PickArgs = make_args(&[
            ("values", json!("a,b,c,d,e")),
            ("count", json!(3)),
            ("seed", json!(42)),
        ]);
        assert_eq!(cmd_pick(&a, None).unwrap(), cmd_pick(&b, None).unwrap());
    }

    #[test]
    fn shuffle_randomizes_order() {
        let input = json_input(&[
            r#"{"v":1}"#,
            r#"{"v":2}"#,
            r#"{"v":3}"#,
            r#"{"v":4}"#,
            r#"{"v":5}"#,
        ]);
        let args: ShuffleArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_shuffle(&args, input).unwrap();
        assert_eq!(result.len(), 5);
        let mut vals: Vec<i64> = result.iter().map(|r| r["v"].as_i64().unwrap()).collect();
        vals.sort();
        assert_eq!(vals, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn shuffle_seed_determinism() {
        let mk_input = || {
            json_input(&[
                r#"{"v":1}"#,
                r#"{"v":2}"#,
                r#"{"v":3}"#,
                r#"{"v":4}"#,
                r#"{"v":5}"#,
            ])
        };
        let a: ShuffleArgs = make_args(&[("seed", json!(42))]);
        let b: ShuffleArgs = make_args(&[("seed", json!(42))]);
        assert_eq!(
            cmd_shuffle(&a, mk_input()).unwrap(),
            cmd_shuffle(&b, mk_input()).unwrap()
        );
    }

    #[test]
    fn shuffle_empty_input() {
        let input: Option<Box<dyn BufRead>> =
            Some(Box::new(std::io::Cursor::new(Vec::<u8>::new())));
        let args: ShuffleArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_shuffle(&args, input).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn pick_no_source_error() {
        let args: PickArgs = make_args(&[]);
        assert!(cmd_pick(&args, None).is_err());
    }

    #[test]
    fn shuffle_no_input_error() {
        let args: ShuffleArgs = make_args(&[]);
        assert!(cmd_shuffle(&args, None).is_err());
    }
}
