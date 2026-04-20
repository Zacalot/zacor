use crate::args::DomainArgs;
use crate::models::*;
use crate::wordnet::WordNet;
use serde_json::Value;
use zacor_package::json;

pub fn cmd_domain(args: &DomainArgs) -> Result<Vec<Value>, String> {
    let wn = WordNet::embedded();
    let count_limit = args.count.map(|c| c as usize);

    match args.domain.as_deref() {
        Some(domain_str) => list_words_in_domain(wn, domain_str, count_limit),
        None => list_all_domains(wn),
    }
}

fn list_all_domains(wn: &WordNet) -> Result<Vec<Value>, String> {
    let mut counts = [0u32; DOMAIN_COUNT];

    for synset in wn.all_pos_synsets() {
        let d = synset.domain as usize;
        if d < DOMAIN_COUNT {
            counts[d] += 1;
        }
    }

    let mut results = Vec::new();
    for (i, name) in all_domain_names().iter().enumerate() {
        if counts[i] > 0 {
            // Determine POS from domain name prefix
            let pos = if name.starts_with("noun.") {
                "noun"
            } else if name.starts_with("verb.") {
                "verb"
            } else if name.starts_with("adj.") {
                "adj"
            } else if name.starts_with("adv.") {
                "adv"
            } else {
                ""
            };
            results.push(json!({
                "value": name,
                "count": counts[i],
                "pos": pos,
            }));
        }
    }
    Ok(results)
}

fn list_words_in_domain(
    wn: &WordNet,
    domain_str: &str,
    count_limit: Option<usize>,
) -> Result<Vec<Value>, String> {
    let domain_num = match_domain(domain_str)
        .ok_or_else(|| format!("unknown domain: {domain_str}"))?;

    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let limit = count_limit.unwrap_or(usize::MAX);

    for synset in wn.all_pos_synsets() {
        if synset.domain != domain_num {
            continue;
        }
        for ws in &synset.word_senses {
            let key = ws.lemma.to_lowercase();
            if seen.insert(key.clone()) {
                results.push(json!({
                    "value": key,
                    "definition": synset.definition,
                    "pos": synset.pos.to_string(),
                }));
                if results.len() >= limit {
                    return Ok(results);
                }
            }
        }
    }

    Ok(results)
}

#[cfg(all(test, feature = "embedded-data"))]
mod tests {
    use super::*;
    use crate::args::DomainArgs;

    fn make_args(domain: Option<&str>, count: Option<i64>) -> DomainArgs {
        DomainArgs {
            domain: domain.map(|s| s.to_string()),
            count,
        }
    }

    #[test]
    fn test_list_all_domains() {
        let args = make_args(None, None);
        let results = cmd_domain(&args).unwrap();
        assert!(!results.is_empty());
        // Should have count field
        assert!(results[0]["count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn test_list_words_in_domain() {
        let args = make_args(Some("noun.person"), None);
        let results = cmd_domain(&args).unwrap();
        assert!(!results.is_empty(), "noun.person should have words");
    }

    #[test]
    fn test_count_limit() {
        let args = make_args(Some("noun.person"), Some(10));
        let results = cmd_domain(&args).unwrap();
        assert!(results.len() <= 10);
    }

    #[test]
    fn test_flexible_naming() {
        let dot = cmd_domain(&make_args(Some("noun.person"), Some(5))).unwrap();
        let under = cmd_domain(&make_args(Some("noun_person"), Some(5))).unwrap();
        let upper = cmd_domain(&make_args(Some("Noun.Person"), Some(5))).unwrap();
        assert_eq!(dot.len(), under.len());
        assert_eq!(dot.len(), upper.len());
    }
}
