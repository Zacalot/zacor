use regex::Regex;
use serde_json::Value;

use crate::parser::{self, BinOp, CmpOp, Expr, Predicate};

/// Evaluate a predicate expression string against a JSON record.
/// Returns `true` if the record matches, `false` otherwise.
pub fn eval_predicate(expr: &str, record: &Value) -> Result<bool, String> {
    let pred = parser::parse_predicate(expr)?;
    eval_pred(&pred, record)
}

/// Evaluate a value expression string against a JSON record.
/// Returns the computed JSON value.
pub fn eval_value(expr: &str, record: &Value) -> Result<Value, String> {
    let ast = parser::parse_value_expr(expr)?;
    eval_expr(&ast, record)
}

fn eval_pred(pred: &Predicate, record: &Value) -> Result<bool, String> {
    match pred {
        Predicate::Comparison(left, op, right) => {
            let lv = eval_expr(left, record)?;
            let rv = eval_expr(right, record)?;
            Ok(compare(&lv, op, &rv))
        }
        Predicate::And(a, b) => Ok(eval_pred(a, record)? && eval_pred(b, record)?),
        Predicate::Or(a, b) => Ok(eval_pred(a, record)? || eval_pred(b, record)?),
        Predicate::Not(inner) => Ok(!eval_pred(inner, record)?),
    }
}

fn eval_expr(expr: &Expr, record: &Value) -> Result<Value, String> {
    match expr {
        Expr::Number(n) => Ok(Value::from(*n)),
        Expr::Str(s) => Ok(Value::String(s.clone())),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Null => Ok(Value::Null),
        Expr::Field(parts) => Ok(resolve_field(record, parts)),
        Expr::BinOp(left, op, right) => {
            let lv = eval_expr(left, record)?;
            let rv = eval_expr(right, record)?;
            eval_binop(&lv, op, &rv)
        }
        Expr::Func(name, args) => eval_func(name, args, record),
    }
}

fn resolve_field(record: &Value, parts: &[String]) -> Value {
    let mut current = record;
    for part in parts {
        match current {
            Value::Object(map) => {
                current = match map.get(part) {
                    Some(v) => v,
                    None => return Value::Null,
                };
            }
            _ => return Value::Null,
        }
    }
    current.clone()
}

fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

fn compare(left: &Value, op: &CmpOp, right: &Value) -> bool {
    // Handle null comparisons
    if left.is_null() || right.is_null() {
        return match op {
            CmpOp::Eq => left.is_null() && right.is_null(),
            CmpOp::Ne => !(left.is_null() && right.is_null()),
            _ => false,
        };
    }

    match op {
        CmpOp::Eq => values_equal(left, right),
        CmpOp::Ne => !values_equal(left, right),
        CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le => {
            if let (Some(a), Some(b)) = (to_f64(left), to_f64(right)) {
                match op {
                    CmpOp::Gt => a > b,
                    CmpOp::Lt => a < b,
                    CmpOp::Ge => a >= b,
                    CmpOp::Le => a <= b,
                    _ => unreachable!(),
                }
            } else {
                let a = to_string(left);
                let b = to_string(right);
                match op {
                    CmpOp::Gt => a > b,
                    CmpOp::Lt => a < b,
                    CmpOp::Ge => a >= b,
                    CmpOp::Le => a <= b,
                    _ => unreachable!(),
                }
            }
        }
        CmpOp::Match => {
            let s = to_string(left);
            let pattern = to_string(right);
            Regex::new(&pattern)
                .map(|re| re.is_match(&s))
                .unwrap_or(false)
        }
        CmpOp::NotMatch => {
            let s = to_string(left);
            let pattern = to_string(right);
            Regex::new(&pattern)
                .map(|re| !re.is_match(&s))
                .unwrap_or(true)
        }
        CmpOp::Contains => {
            let haystack = to_string(left);
            let needle = to_string(right);
            haystack.contains(&needle)
        }
        CmpOp::StartsWith => {
            let s = to_string(left);
            let prefix = to_string(right);
            s.starts_with(&prefix)
        }
        CmpOp::EndsWith => {
            let s = to_string(left);
            let suffix = to_string(right);
            s.ends_with(&suffix)
        }
        CmpOp::In => {
            if let Value::Array(arr) = right {
                arr.iter().any(|item| values_equal(left, item))
            } else {
                false
            }
        }
        CmpOp::NotIn => {
            if let Value::Array(arr) = right {
                !arr.iter().any(|item| values_equal(left, item))
            } else {
                true
            }
        }
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    // Try numeric comparison first
    if let (Some(x), Some(y)) = (to_f64(a), to_f64(b)) {
        if a.is_number() && b.is_number() {
            return (x - y).abs() < f64::EPSILON;
        }
    }
    a == b
}

fn eval_binop(left: &Value, op: &BinOp, right: &Value) -> Result<Value, String> {
    let a = to_f64(left).ok_or_else(|| format!("cannot use {:?} in arithmetic", left))?;
    let b = to_f64(right).ok_or_else(|| format!("cannot use {:?} in arithmetic", right))?;
    let result = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b == 0.0 {
                return Err("division by zero".into());
            }
            a / b
        }
        BinOp::Mod => {
            if b == 0.0 {
                return Err("modulo by zero".into());
            }
            a % b
        }
    };
    Ok(Value::from(result))
}

fn eval_func(name: &str, args: &[Expr], record: &Value) -> Result<Value, String> {
    match name {
        "upper" => {
            check_arity(name, args, 1)?;
            let v = eval_expr(&args[0], record)?;
            Ok(Value::String(to_string(&v).to_uppercase()))
        }
        "lower" => {
            check_arity(name, args, 1)?;
            let v = eval_expr(&args[0], record)?;
            Ok(Value::String(to_string(&v).to_lowercase()))
        }
        "len" => {
            check_arity(name, args, 1)?;
            let v = eval_expr(&args[0], record)?;
            match &v {
                Value::String(s) => Ok(Value::from(s.len() as f64)),
                Value::Array(a) => Ok(Value::from(a.len() as f64)),
                _ => Ok(Value::from(to_string(&v).len() as f64)),
            }
        }
        "trim" => {
            check_arity(name, args, 1)?;
            let v = eval_expr(&args[0], record)?;
            Ok(Value::String(to_string(&v).trim().to_string()))
        }
        "abs" => {
            check_arity(name, args, 1)?;
            let v = eval_expr(&args[0], record)?;
            let n = to_f64(&v).ok_or("abs: expected number")?;
            Ok(Value::from(n.abs()))
        }
        "round" => {
            check_arity(name, args, 1)?;
            let v = eval_expr(&args[0], record)?;
            let n = to_f64(&v).ok_or("round: expected number")?;
            Ok(Value::from(n.round()))
        }
        "floor" => {
            check_arity(name, args, 1)?;
            let v = eval_expr(&args[0], record)?;
            let n = to_f64(&v).ok_or("floor: expected number")?;
            Ok(Value::from(n.floor()))
        }
        "ceil" => {
            check_arity(name, args, 1)?;
            let v = eval_expr(&args[0], record)?;
            let n = to_f64(&v).ok_or("ceil: expected number")?;
            Ok(Value::from(n.ceil()))
        }
        "if" => {
            if args.len() != 3 {
                return Err(format!("if: expected 3 arguments, got {}", args.len()));
            }
            // First arg is evaluated as a predicate-like expression
            // We evaluate it as a value and check truthiness
            let cond = eval_expr(&args[0], record)?;
            let is_true = match &cond {
                Value::Bool(b) => *b,
                Value::Null => false,
                Value::Number(n) => n.as_f64().map_or(false, |v| v != 0.0),
                Value::String(s) => !s.is_empty(),
                _ => true,
            };
            if is_true {
                eval_expr(&args[1], record)
            } else {
                eval_expr(&args[2], record)
            }
        }
        _ => Err(format!("unknown function: {name}")),
    }
}

fn check_arity(name: &str, args: &[Expr], expected: usize) -> Result<(), String> {
    if args.len() != expected {
        return Err(format!(
            "{name}: expected {expected} argument(s), got {}",
            args.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn simple_field_comparison() {
        let record = json!({"size": 1500, "name": "foo"});
        assert!(eval_predicate("size > 1000", &record).unwrap());
        assert!(!eval_predicate("size > 2000", &record).unwrap());
    }

    #[test]
    fn string_equality() {
        let record = json!({"type": "file"});
        assert!(eval_predicate("type == 'file'", &record).unwrap());
        assert!(!eval_predicate("type == 'dir'", &record).unwrap());
    }

    #[test]
    fn boolean_combinators() {
        let record = json!({"size": 200, "type": "dir"});
        assert!(!eval_predicate("size > 100 and type == 'file'", &record).unwrap());
        assert!(eval_predicate("size > 100 or type == 'file'", &record).unwrap());
    }

    #[test]
    fn negation() {
        let record = json!({"type": "file"});
        assert!(eval_predicate("not type == 'dir'", &record).unwrap());
    }

    #[test]
    fn regex_match() {
        let record = json!({"name": "main.rs"});
        assert!(eval_predicate("name =~ '\\.rs$'", &record).unwrap());
        assert!(!eval_predicate("name =~ '\\.py$'", &record).unwrap());
    }

    #[test]
    fn contains_operator() {
        let record = json!({"name": "foobar"});
        assert!(eval_predicate("name contains 'foo'", &record).unwrap());
        assert!(!eval_predicate("name contains 'xyz'", &record).unwrap());
    }

    #[test]
    fn nested_field_access() {
        let record = json!({"info": {"size": 200}});
        assert!(eval_predicate("info.size > 100", &record).unwrap());
    }

    #[test]
    fn missing_field() {
        let record = json!({"name": "foo"});
        assert!(!eval_predicate("missing > 0", &record).unwrap());
    }

    #[test]
    fn invalid_expression() {
        assert!(eval_predicate("size >>", &json!({})).is_err());
    }

    #[test]
    fn arithmetic_on_fields() {
        let record = json!({"price": 10, "qty": 3});
        let result = eval_value("price * qty", &record).unwrap();
        assert_eq!(result, json!(30.0));
    }

    #[test]
    fn string_function_upper() {
        let record = json!({"name": "foo"});
        let result = eval_value("upper(name)", &record).unwrap();
        assert_eq!(result, json!("FOO"));
    }

    #[test]
    fn conditional_expression() {
        let record = json!({"active": true});
        let result = eval_value("if(active, 'yes', 'no')", &record).unwrap();
        assert_eq!(result, json!("yes"));
    }

    #[test]
    fn literal_value() {
        let result = eval_value("'hello'", &json!({})).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn operator_precedence() {
        let record = json!({"a": 1, "b": 2, "c": 3});
        let result = eval_value("a + b * c", &record).unwrap();
        assert_eq!(result, json!(7.0));
    }

    #[test]
    fn len_function() {
        let record = json!({"name": "hello"});
        let result = eval_value("len(name)", &record).unwrap();
        assert_eq!(result, json!(5.0));
    }

    #[test]
    fn if_with_comparison() {
        let record = json!({"score": 95});
        // if function takes a value expression for cond, so we test truthiness
        let result = eval_value("if(score, 'A', 'B')", &record).unwrap();
        assert_eq!(result, json!("A"));
    }

    #[test]
    fn starts_with_ends_with() {
        let record = json!({"name": "foobar"});
        assert!(eval_predicate("name starts-with 'foo'", &record).unwrap());
        assert!(eval_predicate("name ends-with 'bar'", &record).unwrap());
        assert!(!eval_predicate("name starts-with 'bar'", &record).unwrap());
    }

    #[test]
    fn comparison_operators() {
        let record = json!({"a": 5, "b": 10});
        assert!(eval_predicate("a == 5", &record).unwrap());
        assert!(eval_predicate("a != 10", &record).unwrap());
        assert!(eval_predicate("a < 10", &record).unwrap());
        assert!(eval_predicate("a <= 5", &record).unwrap());
        assert!(eval_predicate("b >= 10", &record).unwrap());
        assert!(eval_predicate("b > 5", &record).unwrap());
    }

    #[test]
    fn null_comparison() {
        let record = json!({"a": null, "b": 1});
        assert!(eval_predicate("a == null", &record).unwrap());
        assert!(eval_predicate("a != 1", &record).unwrap());
    }

    #[test]
    fn math_functions() {
        let record = json!({"x": -3.7});
        assert_eq!(eval_value("abs(x)", &record).unwrap(), json!(3.7));
        assert_eq!(eval_value("round(x)", &record).unwrap(), json!(-4.0));
        assert_eq!(eval_value("floor(x)", &record).unwrap(), json!(-4.0));
        assert_eq!(eval_value("ceil(x)", &record).unwrap(), json!(-3.0));
    }

    #[test]
    fn trim_and_lower() {
        let record = json!({"s": "  HELLO  "});
        assert_eq!(eval_value("trim(s)", &record).unwrap(), json!("HELLO"));
        assert_eq!(eval_value("lower(s)", &record).unwrap(), json!("  hello  "));
    }
}
