use serde_json::Value;
use std::io::BufRead;

zacor_package::include_args!();

pub fn first(
    count: i64,
    input: Box<dyn BufRead>,
) -> Result<Vec<Value>, String> {
    let n = count.max(0) as usize;
    let records = zacor_package::parse_records(input)?;
    Ok(records.into_iter().take(n).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn input(s: &str) -> Box<dyn BufRead> {
        Box::new(Cursor::new(s.to_string().into_bytes()))
    }

    #[test]
    fn take_first_3() {
        let data = r#"[{"a":1},{"a":2},{"a":3},{"a":4},{"a":5}]"#;
        let result = first(3, input(data)).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["a"], 1);
        assert_eq!(result[2]["a"], 3);
    }

    #[test]
    fn default_count() {
        let data = r#"[{"a":1},{"a":2}]"#;
        let result = first(1, input(data)).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn count_exceeds_input() {
        let data = r#"[{"a":1},{"a":2}]"#;
        let result = first(10, input(data)).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn empty_input() {
        let result = first(5, input("")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn jsonl_input() {
        let data = "{\"a\":1}\n{\"a\":2}\n{\"a\":3}";
        let result = first(2, input(data)).unwrap();
        assert_eq!(result.len(), 2);
    }
}
