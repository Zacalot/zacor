#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Identifiers and literals
    Ident(String),
    Number(f64),
    Str(String),
    Bool(bool),
    Null,

    // Comparison operators
    Eq,       // ==
    Ne,       // !=
    Gt,       // >
    Lt,       // <
    Ge,       // >=
    Le,       // <=
    Match,    // =~
    NotMatch, // !~

    // Word operators
    And,
    Or,
    Not,
    Contains,
    StartsWith,
    EndsWith,
    In,
    NotIn,

    // Arithmetic
    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    // Delimiters
    LParen,
    RParen,
    Comma,
    Dot,

    Eof,
}

pub struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            let is_eof = tok == Token::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let ch = self.input.get(self.pos).copied()?;
        self.pos += 1;
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, String> {
        self.skip_whitespace();

        let Some(ch) = self.peek() else {
            return Ok(Token::Eof);
        };

        match ch {
            b'(' => {
                self.advance();
                Ok(Token::LParen)
            }
            b')' => {
                self.advance();
                Ok(Token::RParen)
            }
            b',' => {
                self.advance();
                Ok(Token::Comma)
            }
            b'+' => {
                self.advance();
                Ok(Token::Plus)
            }
            b'-' => {
                // Check if this is a negative number
                if self.pos + 1 < self.input.len() && self.input[self.pos + 1].is_ascii_digit() {
                    self.read_number()
                } else {
                    self.advance();
                    Ok(Token::Minus)
                }
            }
            b'*' => {
                self.advance();
                Ok(Token::Star)
            }
            b'/' => {
                self.advance();
                Ok(Token::Slash)
            }
            b'%' => {
                self.advance();
                Ok(Token::Percent)
            }
            b'.' => {
                self.advance();
                Ok(Token::Dot)
            }

            b'=' => {
                self.advance();
                match self.peek() {
                    Some(b'=') => {
                        self.advance();
                        Ok(Token::Eq)
                    }
                    Some(b'~') => {
                        self.advance();
                        Ok(Token::Match)
                    }
                    _ => Err("expected '=' or '~' after '='".into()),
                }
            }

            b'!' => {
                self.advance();
                match self.peek() {
                    Some(b'=') => {
                        self.advance();
                        Ok(Token::Ne)
                    }
                    Some(b'~') => {
                        self.advance();
                        Ok(Token::NotMatch)
                    }
                    _ => Err("expected '=' or '~' after '!'".into()),
                }
            }

            b'>' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(Token::Ge)
                } else {
                    Ok(Token::Gt)
                }
            }

            b'<' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(Token::Le)
                } else {
                    Ok(Token::Lt)
                }
            }

            b'\'' => self.read_string(),

            b'0'..=b'9' => self.read_number(),

            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.read_ident_or_keyword(),

            _ => Err(format!("unexpected character: '{}'", ch as char)),
        }
    }

    fn read_string(&mut self) -> Result<Token, String> {
        self.advance(); // skip opening quote
        let mut s = String::new();
        loop {
            match self.advance() {
                Some(b'\\') => match self.advance() {
                    Some(b'\'') => s.push('\''),
                    Some(b'\\') => s.push('\\'),
                    Some(b'n') => s.push('\n'),
                    Some(b't') => s.push('\t'),
                    Some(c) => {
                        s.push('\\');
                        s.push(c as char);
                    }
                    None => return Err("unterminated string".into()),
                },
                Some(b'\'') => return Ok(Token::Str(s)),
                Some(c) => s.push(c as char),
                None => return Err("unterminated string".into()),
            }
        }
    }

    fn read_number(&mut self) -> Result<Token, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.advance();
        }
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        if self.peek() == Some(b'.') {
            self.advance();
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let s = std::str::from_utf8(&self.input[start..self.pos]).map_err(|_| "invalid number")?;
        let n: f64 = s.parse().map_err(|_| format!("invalid number: {s}"))?;
        Ok(Token::Number(n))
    }

    fn read_ident_or_keyword(&mut self) -> Result<Token, String> {
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'-' {
                self.advance();
            } else {
                break;
            }
        }
        let word =
            std::str::from_utf8(&self.input[start..self.pos]).map_err(|_| "invalid identifier")?;

        let tok = match word {
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            "contains" => Token::Contains,
            "starts-with" => Token::StartsWith,
            "ends-with" => Token::EndsWith,
            "in" => Token::In,
            "not-in" => Token::NotIn,
            "true" => Token::Bool(true),
            "false" => Token::Bool(false),
            "null" => Token::Null,
            _ => Token::Ident(word.to_string()),
        };
        Ok(tok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<Token> {
        Lexer::new(input).tokenize().unwrap()
    }

    #[test]
    fn simple_comparison() {
        let tokens = lex("size > 1000");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("size".into()),
                Token::Gt,
                Token::Number(1000.0),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn string_literal() {
        let tokens = lex("type == 'file'");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("type".into()),
                Token::Eq,
                Token::Str("file".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn boolean_combinators() {
        let tokens = lex("a > 1 and b == 'x'");
        assert_eq!(tokens[3], Token::And);
    }

    #[test]
    fn regex_match() {
        let tokens = lex("name =~ '\\.rs$'");
        assert_eq!(tokens[1], Token::Match);
    }

    #[test]
    fn nested_field() {
        let tokens = lex("info.size > 100");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("info".into()),
                Token::Dot,
                Token::Ident("size".into()),
                Token::Gt,
                Token::Number(100.0),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn function_call() {
        let tokens = lex("upper(name)");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("upper".into()),
                Token::LParen,
                Token::Ident("name".into()),
                Token::RParen,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn arithmetic() {
        let tokens = lex("price * qty + 1");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("price".into()),
                Token::Star,
                Token::Ident("qty".into()),
                Token::Plus,
                Token::Number(1.0),
                Token::Eof,
            ]
        );
    }
}
