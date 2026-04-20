use crate::args::LookupArgs;
use crate::models::*;
use crate::wordnet::WordNet;
use serde_json::Value;
use zacor_package::json;

pub fn cmd_lookup(args: &LookupArgs) -> Result<Vec<Value>, String> {
    let wn = WordNet::embedded();
    let word = &args.word;
    let pos_filter = args.pos.as_deref().and_then(POS::from_str_loose);

    let normalized = word.to_lowercase().replace(' ', "_");
    let senses = match wn.lookup_word(&normalized) {
        Some(s) => s,
        None => return Ok(vec![]),
    };

    let mut results: Vec<(u32, Value)> = Vec::new();
    let mut sense_num = 0u32;

    for &(pos, id) in senses {
        if let Some(filter) = pos_filter {
            if pos != filter {
                continue;
            }
        }
        if let Some(synset) = wn.get_synset(pos, id) {
            sense_num += 1;
            // Find the frequency for this specific word in this synset
            let freq = synset
                .word_senses
                .iter()
                .find(|ws| ws.lemma.to_lowercase() == normalized)
                .map(|ws| ws.frequency)
                .unwrap_or(0);

            let examples: Vec<&str> = synset.examples.iter().map(|s| s.as_str()).collect();

            results.push((
                freq,
                json!({
                    "value": word.to_lowercase().replace(' ', "_"),
                    "pos": pos.to_string(),
                    "domain": domain_name(synset.domain),
                    "definition": synset.definition,
                    "examples": examples,
                    "frequency": freq,
                    "sense": sense_num,
                }),
            ));
        }
    }

    // Sort by frequency descending
    results.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(results.into_iter().map(|(_, v)| v).collect())
}

#[cfg(all(test, feature = "embedded-data"))]
mod tests {
    use super::*;
    use crate::args::LookupArgs;

    fn make_args(word: &str, pos: Option<&str>) -> LookupArgs {
        LookupArgs {
            word: word.to_string(),
            pos: pos.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_lookup_single_sense() {
        let args = make_args("alchemist", None);
        let results = cmd_lookup(&args).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0]["pos"], "noun");
    }

    #[test]
    fn test_lookup_multi_sense() {
        let args = make_args("bank", None);
        let results = cmd_lookup(&args).unwrap();
        assert!(results.len() > 1, "bank should have multiple senses");
    }

    #[test]
    fn test_lookup_not_found() {
        let args = make_args("xyzzyplugh", None);
        let results = cmd_lookup(&args).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_lookup_case_insensitive() {
        let lower = cmd_lookup(&make_args("brave", None)).unwrap();
        let upper = cmd_lookup(&make_args("Brave", None)).unwrap();
        assert_eq!(lower.len(), upper.len());
    }

    #[test]
    fn test_lookup_pos_filter() {
        let args = make_args("run", Some("verb"));
        let results = cmd_lookup(&args).unwrap();
        for r in &results {
            assert_eq!(r["pos"], "verb");
        }
    }

    #[test]
    fn test_lookup_multi_word() {
        let args = make_args("ice cream", None);
        let results = cmd_lookup(&args).unwrap();
        assert!(!results.is_empty(), "ice cream should be found");
    }
}
