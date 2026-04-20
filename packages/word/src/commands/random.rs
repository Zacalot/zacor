use crate::args::RandomArgs;
use crate::models::*;
use crate::wordnet::WordNet;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde_json::Value;
use zacor_package::json;

pub fn cmd_random(args: &RandomArgs) -> Result<Vec<Value>, String> {
    let wn = WordNet::embedded();
    let count = args.count.unwrap_or(1).max(1) as usize;
    let pos_filter = args.pos.as_deref().and_then(POS::from_str_loose);
    let domain_filter = args.domain.as_deref().and_then(match_domain);

    let mut rng: StdRng = match args.seed {
        Some(s) => StdRng::seed_from_u64(s as u64),
        None => StdRng::from_entropy(),
    };

    // Build candidate pool: (word, pos, domain, definition)
    let mut pool: Vec<(&str, POS, u8, &str)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for synset in wn.all_pos_synsets() {
        if let Some(filter) = pos_filter {
            if synset.pos != filter {
                continue;
            }
        }
        if let Some(domain_num) = domain_filter {
            if synset.domain != domain_num {
                continue;
            }
        }
        for ws in &synset.word_senses {
            let key = (ws.lemma.as_str(), synset.pos as u8);
            if seen.insert(key) {
                pool.push((&ws.lemma, synset.pos, synset.domain, &synset.definition));
            }
        }
    }

    if pool.is_empty() {
        return Ok(vec![]);
    }

    pool.shuffle(&mut rng);
    let selected = &pool[..count.min(pool.len())];

    Ok(selected
        .iter()
        .map(|&(word, pos, domain, definition)| {
            json!({
                "value": word.to_lowercase(),
                "pos": pos.to_string(),
                "domain": domain_name(domain),
                "definition": definition,
            })
        })
        .collect())
}

#[cfg(all(test, feature = "embedded-data"))]
mod tests {
    use super::*;
    use crate::args::RandomArgs;

    fn make_args(
        pos: Option<&str>,
        domain: Option<&str>,
        count: Option<i64>,
        seed: Option<f64>,
    ) -> RandomArgs {
        RandomArgs {
            pos: pos.map(|s| s.to_string()),
            domain: domain.map(|s| s.to_string()),
            count,
            seed,
        }
    }

    #[test]
    fn test_unfiltered() {
        let results = cmd_random(&make_args(None, None, Some(1), Some(42.0))).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_pos_filter() {
        let results = cmd_random(&make_args(Some("adj"), None, Some(5), Some(42.0))).unwrap();
        for r in &results {
            assert_eq!(r["pos"], "adj");
        }
    }

    #[test]
    fn test_domain_filter() {
        let results =
            cmd_random(&make_args(None, Some("noun.person"), Some(5), Some(42.0))).unwrap();
        for r in &results {
            assert_eq!(r["domain"], "noun.person");
        }
    }

    #[test]
    fn test_count() {
        let results = cmd_random(&make_args(None, None, Some(10), Some(42.0))).unwrap();
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn test_seed_determinism() {
        let r1 = cmd_random(&make_args(None, None, Some(3), Some(42.0))).unwrap();
        let r2 = cmd_random(&make_args(None, None, Some(3), Some(42.0))).unwrap();
        assert_eq!(r1, r2, "same seed should produce same results");
    }
}
