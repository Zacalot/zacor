use fasteval::{Compiler, Evaler};
use serde::Serialize;

zacor_package::include_args!();

#[derive(Serialize)]
pub struct CalcRecord {
    pub value: f64,
}

pub fn calc(args: args::DefaultArgs) -> Result<CalcRecord, String> {
    let expr = args.expr.as_deref().unwrap_or("");
    if expr.trim().is_empty() {
        return Err("calc: expression is empty".into());
    }

    let parser = fasteval::Parser::new();
    let mut slab = fasteval::Slab::new();
    let compiled = parser
        .parse(expr, &mut slab.ps)
        .map_err(|e| format!("calc: {e}"))?
        .from(&slab.ps)
        .compile(&slab.ps, &mut slab.cs);
    let val = compiled
        .eval(&slab, &mut |name: &str, args: Vec<f64>| -> Option<f64> {
            match name {
                "pi" => Some(std::f64::consts::PI),
                "e" => Some(std::f64::consts::E),
                "sqrt" => args.first().map(|v| v.sqrt()),
                "abs" => args.first().map(|v| v.abs()),
                "ln" => args.first().map(|v| v.ln()),
                "exp" => args.first().map(|v| v.exp()),
                "sign" => args.first().map(|v| v.signum()),
                _ => None,
            }
        })
        .map_err(|e| format!("calc: {e}"))?;

    Ok(CalcRecord { value: val })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zacor_package::FromArgs;
    use std::collections::BTreeMap;
    use serde_json::json;

    fn eval(expr: &str) -> Result<f64, String> {
        let map: BTreeMap<String, _> = [("expr".into(), json!(expr))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        Ok(calc(args)?.value)
    }

    // 2.1 Basic arithmetic
    #[test]
    fn addition() {
        assert_eq!(eval("2 + 3").unwrap(), 5.0);
    }

    #[test]
    fn subtraction() {
        assert_eq!(eval("10 - 4").unwrap(), 6.0);
    }

    #[test]
    fn multiplication() {
        assert_eq!(eval("3 * 4").unwrap(), 12.0);
    }

    #[test]
    fn division() {
        assert_eq!(eval("15 / 4").unwrap(), 3.75);
    }

    #[test]
    fn modulo() {
        assert_eq!(eval("17 % 5").unwrap(), 2.0);
    }

    #[test]
    fn exponentiation() {
        assert_eq!(eval("2^10").unwrap(), 1024.0);
    }

    #[test]
    fn parentheses() {
        assert_eq!(eval("(2 + 3) * 4").unwrap(), 20.0);
    }

    #[test]
    fn negation() {
        assert_eq!(eval("-5 + 3").unwrap(), -2.0);
    }

    // 2.2 Trig functions and constants
    #[test]
    fn sin_pi() {
        assert!((eval("sin(pi())").unwrap()).abs() < 1e-10);
    }

    #[test]
    fn cos_zero() {
        assert_eq!(eval("cos(0)").unwrap(), 1.0);
    }

    #[test]
    fn tan() {
        let val = eval("tan(0)").unwrap();
        assert!(val.abs() < 1e-10);
    }

    #[test]
    fn pi_constant_bare() {
        let val = eval("pi").unwrap();
        assert!((val - std::f64::consts::PI).abs() < 1e-10);
    }

    #[test]
    fn e_constant_bare() {
        let val = eval("e").unwrap();
        assert!((val - std::f64::consts::E).abs() < 1e-10);
    }

    #[test]
    fn two_pi() {
        let val = eval("2 * pi").unwrap();
        assert!((val - 2.0 * std::f64::consts::PI).abs() < 1e-10);
    }

    // 2.3 Math functions
    #[test]
    fn sqrt() {
        assert_eq!(eval("sqrt(144)").unwrap(), 12.0);
    }

    #[test]
    fn abs() {
        assert_eq!(eval("abs(-7)").unwrap(), 7.0);
    }

    #[test]
    fn ln_e() {
        let val = eval("ln(e())").unwrap();
        assert!((val - 1.0).abs() < 1e-10);
    }

    #[test]
    fn log_base10() {
        let val = eval("log(100)").unwrap();
        assert!((val - 2.0).abs() < 1e-10);
    }

    #[test]
    fn floor() {
        assert_eq!(eval("floor(3.7)").unwrap(), 3.0);
    }

    #[test]
    fn ceil() {
        assert_eq!(eval("ceil(3.2)").unwrap(), 4.0);
    }

    #[test]
    fn round() {
        assert_eq!(eval("round(3.5)").unwrap(), 4.0);
    }

    #[test]
    fn min_max() {
        assert_eq!(eval("min(3, 7)").unwrap(), 3.0);
        assert_eq!(eval("max(3, 7)").unwrap(), 7.0);
    }

    // 2.4 Comparisons and logical operators
    #[test]
    fn comparison_gt() {
        assert_eq!(eval("3 > 2").unwrap(), 1.0);
    }

    #[test]
    fn comparison_lt() {
        assert_eq!(eval("2 < 3").unwrap(), 1.0);
    }

    #[test]
    fn logical_and() {
        assert_eq!(eval("3 > 2 && 1 < 0").unwrap(), 0.0);
    }

    #[test]
    fn logical_or() {
        assert_eq!(eval("3 > 2 || 1 < 0").unwrap(), 1.0);
    }

    // 2.5 Error cases
    #[test]
    fn empty_expression() {
        assert!(eval("").is_err());
    }

    #[test]
    fn whitespace_only() {
        assert!(eval("   ").is_err());
    }

    #[test]
    fn malformed_expression() {
        assert!(eval("2 +* 3").is_err());
    }
}
