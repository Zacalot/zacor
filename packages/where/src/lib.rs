use serde_json::Value;
use std::io::BufRead;

zacor_package::include_args!();

pub fn filter(
    expr: &str,
    input: Box<dyn BufRead>,
) -> Result<Vec<Value>, String> {
    if expr.is_empty() {
        return Err("where: expression is required".into());
    }

    let records = zacor_package::parse_records(input)?;
    let mut output = Vec::new();

    for record in &records {
        if zr_expr::eval_predicate(expr, record)? {
            output.push(record.clone());
        }
    }

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
    fn basic_comparison() {
        let data = r#"[{"size":100},{"size":2000},{"size":50}]"#;
        let result = filter("size > 500", input(data)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["size"], 2000);
    }

    #[test]
    fn string_matching() {
        let data = r#"[{"type":"file"},{"type":"dir"},{"type":"file"}]"#;
        let result = filter("type == 'file'", input(data)).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn compound_predicate() {
        let data = r#"[{"size":200,"type":"file"},{"size":50,"type":"file"},{"size":200,"type":"dir"}]"#;
        let result = filter("size > 100 and type == 'file'", input(data)).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn no_matches() {
        let data = r#"[{"a":1},{"a":2}]"#;
        let result = filter("a > 100", input(data)).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn empty_input() {
        let result = filter("a > 0", input("")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn regex_match() {
        let data = r#"[{"name":"main.rs"},{"name":"readme.md"}]"#;
        let result = filter("name =~ '\\.rs$'", input(data)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "main.rs");
    }

    #[test]
    fn missing_fields() {
        let data = r#"[{"name":"foo"},{"name":"bar","size":100}]"#;
        let result = filter("size > 50", input(data)).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn missing_expression_error() {
        assert!(filter("", input("[]")).is_err());
    }
}
