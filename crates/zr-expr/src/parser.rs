use crate::lexer::Token;

/// A value expression (arithmetic, field refs, functions, literals).
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Str(String),
    Bool(bool),
    Null,
    Field(Vec<String>), // e.g. ["info", "size"] for info.size
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    Func(String, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

/// A predicate expression (boolean result).
#[derive(Debug, Clone, PartialEq)]
pub enum Predicate {
    Comparison(Expr, CmpOp, Expr),
    And(Box<Predicate>, Box<Predicate>),
    Or(Box<Predicate>, Box<Predicate>),
    Not(Box<Predicate>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    Match,
    NotMatch,
    Contains,
    StartsWith,
    EndsWith,
    In,
    NotIn,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        let tok = self.advance();
        if &tok == expected {
            Ok(())
        } else {
            Err(format!("expected {expected:?}, got {tok:?}"))
        }
    }

    // ─── Predicate parsing ──────────────────────────────────────────

    pub fn parse_predicate(&mut self) -> Result<Predicate, String> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Predicate, String> {
        let mut left = self.parse_and()?;
        while *self.peek() == Token::Or {
            self.advance();
            let right = self.parse_and()?;
            left = Predicate::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Predicate, String> {
        let mut left = self.parse_not()?;
        while *self.peek() == Token::And {
            self.advance();
            let right = self.parse_not()?;
            left = Predicate::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Predicate, String> {
        if *self.peek() == Token::Not {
            self.advance();
            let inner = self.parse_not()?;
            return Ok(Predicate::Not(Box::new(inner)));
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<Predicate, String> {
        if *self.peek() == Token::LParen {
            self.advance();
            let pred = self.parse_predicate()?;
            self.expect(&Token::RParen)?;
            return Ok(pred);
        }

        let left = self.parse_expr()?;
        let op = match self.peek() {
            Token::Eq => CmpOp::Eq,
            Token::Ne => CmpOp::Ne,
            Token::Gt => CmpOp::Gt,
            Token::Lt => CmpOp::Lt,
            Token::Ge => CmpOp::Ge,
            Token::Le => CmpOp::Le,
            Token::Match => CmpOp::Match,
            Token::NotMatch => CmpOp::NotMatch,
            Token::Contains => CmpOp::Contains,
            Token::StartsWith => CmpOp::StartsWith,
            Token::EndsWith => CmpOp::EndsWith,
            Token::In => CmpOp::In,
            Token::NotIn => CmpOp::NotIn,
            other => return Err(format!("expected comparison operator, got {other:?}")),
        };
        self.advance();
        let right = self.parse_expr()?;
        Ok(Predicate::Comparison(left, op, right))
    }

    // ─── Value expression parsing ───────────────────────────────────

    pub fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_add()
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_mul()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_atom()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_atom()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_atom(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Token::Number(n) => {
                self.advance();
                Ok(Expr::Number(n))
            }
            Token::Str(s) => {
                self.advance();
                Ok(Expr::Str(s))
            }
            Token::Bool(b) => {
                self.advance();
                Ok(Expr::Bool(b))
            }
            Token::Null => {
                self.advance();
                Ok(Expr::Null)
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::Ident(name) => {
                self.advance();
                // Check if this is a function call
                if *self.peek() == Token::LParen {
                    self.advance();
                    let mut args = Vec::new();
                    if *self.peek() != Token::RParen {
                        // For 'if' function, first arg is a predicate (parsed as expr)
                        args.push(self.parse_expr()?);
                        while *self.peek() == Token::Comma {
                            self.advance();
                            args.push(self.parse_expr()?);
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Ok(Expr::Func(name, args))
                } else {
                    // Field reference, possibly dotted
                    let mut parts = vec![name];
                    while *self.peek() == Token::Dot {
                        self.advance();
                        match self.advance() {
                            Token::Ident(p) => parts.push(p),
                            other => {
                                return Err(format!("expected field name after '.', got {other:?}"))
                            }
                        }
                    }
                    Ok(Expr::Field(parts))
                }
            }
            other => Err(format!("unexpected token: {other:?}")),
        }
    }
}

/// Parse an expression string into a Predicate AST.
pub fn parse_predicate(input: &str) -> Result<Predicate, String> {
    let tokens = crate::lexer::Lexer::new(input).tokenize()?;
    let mut parser = Parser::new(tokens);
    let pred = parser.parse_predicate()?;
    if *parser.peek() != Token::Eof {
        return Err(format!(
            "unexpected token after expression: {:?}",
            parser.peek()
        ));
    }
    Ok(pred)
}

/// Parse an expression string into a value Expr AST.
pub fn parse_value_expr(input: &str) -> Result<Expr, String> {
    let tokens = crate::lexer::Lexer::new(input).tokenize()?;
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr()?;
    if *parser.peek() != Token::Eof {
        return Err(format!(
            "unexpected token after expression: {:?}",
            parser.peek()
        ));
    }
    Ok(expr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_comparison() {
        let pred = parse_predicate("size > 1000").unwrap();
        assert!(matches!(pred, Predicate::Comparison(
            Expr::Field(f), CmpOp::Gt, Expr::Number(n)
        ) if f == vec!["size"] && n == 1000.0));
    }

    #[test]
    fn compound_and() {
        let pred = parse_predicate("a > 1 and b == 'x'").unwrap();
        assert!(matches!(pred, Predicate::And(_, _)));
    }

    #[test]
    fn not_predicate() {
        let pred = parse_predicate("not active == true").unwrap();
        assert!(matches!(pred, Predicate::Not(_)));
    }

    #[test]
    fn nested_field() {
        let pred = parse_predicate("info.size > 100").unwrap();
        match pred {
            Predicate::Comparison(Expr::Field(f), CmpOp::Gt, _) => {
                assert_eq!(f, vec!["info", "size"]);
            }
            _ => panic!("unexpected: {pred:?}"),
        }
    }

    #[test]
    fn function_call_in_expr() {
        let expr = parse_value_expr("upper(name)").unwrap();
        assert!(matches!(expr, Expr::Func(name, args) if name == "upper" && args.len() == 1));
    }

    #[test]
    fn arithmetic_precedence() {
        let expr = parse_value_expr("a + b * c").unwrap();
        // Should be Add(a, Mul(b, c))
        match expr {
            Expr::BinOp(_, BinOp::Add, right) => {
                assert!(matches!(*right, Expr::BinOp(_, BinOp::Mul, _)));
            }
            _ => panic!("unexpected: {expr:?}"),
        }
    }

    #[test]
    fn malformed_expression_error() {
        assert!(parse_predicate("size >>").is_err());
    }
}
