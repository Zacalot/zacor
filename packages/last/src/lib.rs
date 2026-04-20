use serde_json::Value;
use std::io::BufRead;

zacor_package::include_args!();

pub fn last(
    count: i64,
    input: Box<dyn BufRead>,
) -> Result<Vec<Value>, String> {
    let n = count.max(0) as usize;
    let records = zacor_package::parse_records(input)?;
    let len = records.len();
    let skip = len.saturating_sub(n);
    Ok(records.into_iter().skip(skip).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn input(s: &str) -> Box<dyn BufRead> {
        Box::new(Cursor::new(s.to_string().into_bytes()))
    }

    #[test]
    fn take_last_3() {
        let data = r#"[{"a":1},{"a":2},{"a":3},{"a":4},{"a":5}]"#;
        let result = last(3, input(data)).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["a"], 3);
        assert_eq!(result[2]["a"], 5);
    }

    #[test]
    fn default_count() {
        let data = r#"[{"a":1},{"a":2}]"#;
        let result = last(1, input(data)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["a"], 2);
    }

    #[test]
    fn count_exceeds_input() {
        let data = r#"[{"a":1},{"a":2}]"#;
        let result = last(10, input(data)).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn empty_input() {
        let result = last(5, input("")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn jsonl_input() {
        let data = "{\"a\":1}\n{\"a\":2}\n{\"a\":3}";
        let result = last(2, input(data)).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["a"], 2);
    }
}
