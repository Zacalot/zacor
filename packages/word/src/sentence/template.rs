//! Template parser. Turns a string like
//! `"the {adj} {noun:animal} {verb:motion:3sg}"` into a sequence of literal
//! and slot tokens.
//!
//! Slot syntax: `{pos[:x[:y]]}` where each `x`/`y` is positionally
//! disambiguated — if it parses as a known [`Form`] it's a form, otherwise
//! it's a domain shortname. The shortname is combined with the slot's POS to
//! form a full WordNet domain (e.g. `noun:animal` → `noun.animal`).
//!
//! Escape with backslash to emit literal `{` or `}`: `\{`, `\}`, `\\`.

use crate::models::{POS, match_domain};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Lemma,
    Plural,
    ThirdSingular,
}

#[derive(Debug, Clone)]
pub enum Token {
    Lit(String),
    Slot {
        pos: POS,
        domain: Option<u8>,
        form: Form,
    },
}

pub fn parse(template: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('{') | Some('}') | Some('\\') => {
                    buf.push(chars.next().unwrap());
                }
                _ => buf.push(c),
            }
        } else if c == '{' {
            if !buf.is_empty() {
                tokens.push(Token::Lit(std::mem::take(&mut buf)));
            }
            let mut slot = String::new();
            let mut closed = false;
            for nc in chars.by_ref() {
                if nc == '}' {
                    closed = true;
                    break;
                }
                slot.push(nc);
            }
            if !closed {
                return Err(format!("unterminated slot: {{{slot}"));
            }
            tokens.push(parse_slot(&slot)?);
        } else if c == '}' {
            return Err("unexpected '}' (use \\} to emit a literal)".to_string());
        } else {
            buf.push(c);
        }
    }

    if !buf.is_empty() {
        tokens.push(Token::Lit(buf));
    }
    Ok(tokens)
}

fn parse_form(s: &str) -> Option<Form> {
    match s {
        "pl" => Some(Form::Plural),
        "3sg" => Some(Form::ThirdSingular),
        "sg" | "lemma" => Some(Form::Lemma),
        _ => None,
    }
}

fn parse_slot(s: &str) -> Result<Token, String> {
    let parts: Vec<&str> = s.split(':').map(str::trim).collect();
    if parts.is_empty() || parts[0].is_empty() {
        return Err("empty slot: {}".to_string());
    }
    let pos = POS::from_str_loose(parts[0])
        .ok_or_else(|| format!("unknown POS in slot: '{}'", parts[0]))?;

    let mut domain: Option<u8> = None;
    let mut form: Form = Form::Lemma;

    for part in &parts[1..] {
        if part.is_empty() {
            continue;
        }
        if let Some(f) = parse_form(part) {
            form = f;
        } else {
            let full = format!("{pos}.{part}");
            let d = match_domain(&full)
                .ok_or_else(|| format!("unknown domain '{part}' for {pos}"))?;
            domain = Some(d);
        }
    }

    Ok(Token::Slot { pos, domain, form })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_literal() {
        let t = parse("hello world").unwrap();
        assert_eq!(t.len(), 1);
        matches!(&t[0], Token::Lit(s) if s == "hello world");
    }

    #[test]
    fn parses_simple_slot() {
        let t = parse("{noun}").unwrap();
        assert_eq!(t.len(), 1);
        match &t[0] {
            Token::Slot { pos, domain, form } => {
                assert_eq!(*pos, POS::Noun);
                assert!(domain.is_none());
                assert_eq!(*form, Form::Lemma);
            }
            _ => panic!("expected slot"),
        }
    }

    #[test]
    fn parses_slot_with_domain() {
        let t = parse("{noun:animal}").unwrap();
        match &t[0] {
            Token::Slot { pos, domain, form } => {
                assert_eq!(*pos, POS::Noun);
                assert!(domain.is_some());
                assert_eq!(*form, Form::Lemma);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_slot_with_form() {
        let t = parse("{verb:3sg}").unwrap();
        match &t[0] {
            Token::Slot { pos, domain, form } => {
                assert_eq!(*pos, POS::Verb);
                assert!(domain.is_none());
                assert_eq!(*form, Form::ThirdSingular);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_slot_with_domain_and_form() {
        let t = parse("{verb:motion:3sg}").unwrap();
        match &t[0] {
            Token::Slot { pos, domain, form } => {
                assert_eq!(*pos, POS::Verb);
                assert!(domain.is_some());
                assert_eq!(*form, Form::ThirdSingular);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn mixed_template() {
        let t = parse("the {adj} {noun:animal} runs").unwrap();
        assert_eq!(t.len(), 5); // "the " | adj | " " | noun | " runs"
    }

    #[test]
    fn unterminated_slot_errors() {
        assert!(parse("the {noun").is_err());
    }

    #[test]
    fn unknown_pos_errors() {
        assert!(parse("{xxx}").is_err());
    }

    #[test]
    fn unknown_domain_errors() {
        assert!(parse("{noun:notadomain}").is_err());
    }

    #[test]
    fn escape_braces() {
        let t = parse("\\{literal\\}").unwrap();
        assert_eq!(t.len(), 1);
        match &t[0] {
            Token::Lit(s) => assert_eq!(s, "{literal}"),
            _ => panic!(),
        }
    }
}
