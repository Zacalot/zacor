use crate::args::PatternArgs;
use crate::models::*;
use crate::wordnet::WordNet;
use regex::Regex;
use serde_json::Value;
use zacor_package::json;

pub fn cmd_pattern(args: &PatternArgs) -> Result<Vec<Value>, String> {
    let wn = WordNet::embedded();
    let pattern = &args.pattern;
    let pos_filter = args.pos.as_deref().and_then(POS::from_str_loose);
    let count_limit = args.count.map(|c| c as usize).unwrap_or(usize::MAX);

    // Convert glob pattern to regex
    let regex_str = glob_to_regex(pattern);
    let re = Regex::new(&regex_str).map_err(|e| format!("invalid pattern: {e}"))?;

    let index = wn.index();
    let mut results = Vec::new();

    for (word, senses) in index {
        if !re.is_match(word) {
            continue;
        }

        // Find the first matching sense (optionally filtered by POS)
        for &(pos, id) in senses {
            if let Some(filter) = pos_filter {
                if pos != filter {
                    continue;
                }
            }
            if let Some(synset) = wn.get_synset(pos, id) {
                results.push(json!({
                    "value": word,
                    "pos": pos.to_string(),
                    "definition": synset.definition,
                }));
                break; // one entry per word
            }
        }

        if results.len() >= count_limit {
            break;
        }
    }

    Ok(results)
}

fn glob_to_regex(pattern: &str) -> String {
    let mut regex = String::from("^");
    for c in pattern.to_lowercase().chars() {
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                regex.push('\\');
                regex.push(c);
            }
            _ => regex.push(c),
        }
    }
    regex.push('$');
    regex
}

#[cfg(all(test, feature = "embedded-data"))]
mod tests {
    use super::*;
    use crate::args::PatternArgs;

    fn make_args(pattern: &str, pos: Option<&str>, count: Option<i64>) -> PatternArgs {
        PatternArgs {
            pattern: pattern.to_string(),
            pos: pos.map(|s| s.to_string()),
            count,
        }
    }

    #[test]
    fn test_single_char_wildcard() {
        let results = cmd_pattern(&make_args("b?t", None, None)).unwrap();
        assert!(!results.is_empty());
        for r in &results {
            let v = r["value"].as_str().unwrap();
            assert_eq!(v.len(), 3);
            assert!(v.starts_with('b'));
            assert!(v.ends_with('t'));
        }
    }

    #[test]
    fn test_multi_char_wildcard() {
        let results = cmd_pattern(&make_args("un*able", None, None)).unwrap();
        assert!(!results.is_empty());
        for r in &results {
            let v = r["value"].as_str().unwrap();
            assert!(v.starts_with("un"));
            assert!(v.ends_with("able"));
        }
    }

    #[test]
    fn test_prefix() {
        let results = cmd_pattern(&make_args("psych*", None, None)).unwrap();
        assert!(!results.is_empty());
        for r in &results {
            assert!(r["value"].as_str().unwrap().starts_with("psych"));
        }
    }

    #[test]
    fn test_suffix() {
        let results = cmd_pattern(&make_args("*ology", None, None)).unwrap();
        assert!(!results.is_empty());
        for r in &results {
            assert!(r["value"].as_str().unwrap().ends_with("ology"));
        }
    }

    #[test]
    fn test_pos_filter() {
        let results = cmd_pattern(&make_args("un*", Some("adj"), Some(10))).unwrap();
        for r in &results {
            assert_eq!(r["pos"], "adj");
        }
    }

    #[test]
    fn test_count_limit() {
        let results = cmd_pattern(&make_args("a*", None, Some(20))).unwrap();
        assert!(results.len() <= 20);
    }
}
