use crate::args::RelatedArgs;
use crate::models::*;
use crate::wordnet::WordNet;
use serde_json::Value;
use std::collections::HashSet;
use zacor_package::json;

pub fn cmd_related(args: &RelatedArgs) -> Result<Vec<Value>, String> {
    let wn = WordNet::embedded();
    let word = &args.word;
    let depth = args.depth.unwrap_or(1).max(1) as u32;
    let pos_filter = args.pos.as_deref().and_then(POS::from_str_loose);
    let sense_filter = args.sense.map(|s| s as u32);
    let relation_filter = args.relation.as_deref();

    let is_synonym_only = relation_filter
        .map(|r| r.to_lowercase() == "synonym")
        .unwrap_or(false);

    let relation_types: Option<Vec<RelationType>> = relation_filter.and_then(|r| {
        let types = RelationType::from_filter(r);
        if types.is_empty() { None } else { Some(types) }
    });

    let normalized = word.to_lowercase().replace(' ', "_");
    let senses = match wn.lookup_word(&normalized) {
        Some(s) => s,
        None => return Ok(vec![]),
    };

    // Build senses with frequency for ordering
    let mut sense_entries: Vec<(POS, SynsetId, u32)> = Vec::new();
    let mut sense_num = 0u32;
    for &(pos, id) in senses {
        if let Some(filter) = pos_filter {
            if pos != filter {
                continue;
            }
        }
        sense_num += 1;
        if let Some(wanted) = sense_filter {
            if sense_num != wanted {
                continue;
            }
        }
        let freq = wn
            .get_synset(pos, id)
            .and_then(|s| {
                s.word_senses
                    .iter()
                    .find(|ws| ws.lemma.to_lowercase() == normalized)
            })
            .map(|ws| ws.frequency)
            .unwrap_or(0);
        sense_entries.push((pos, id, freq));
    }
    // Sort by frequency descending so most common sense comes first
    sense_entries.sort_by(|a, b| b.2.cmp(&a.2));
    let selected_senses: Vec<(POS, SynsetId)> =
        sense_entries.into_iter().map(|(p, id, _)| (p, id)).collect();

    let mut results = Vec::new();
    let mut seen = HashSet::new();

    for &(pos, id) in &selected_senses {
        // Synonyms: other words in the same synset
        if is_synonym_only || relation_filter.is_none() {
            if let Some(synset) = wn.get_synset(pos, id) {
                for ws in &synset.word_senses {
                    if ws.lemma.to_lowercase() == normalized {
                        continue;
                    }
                    let key = format!("synonym:{}:{}", ws.lemma.to_lowercase(), id.0);
                    if seen.insert(key) {
                        results.push(json!({
                            "value": ws.lemma.to_lowercase(),
                            "relation": "synonym",
                            "definition": synset.definition,
                            "pos": pos.to_string(),
                            "depth": 1,
                        }));
                    }
                }
            }
        }

        // When synonym filter is active, also collect words from hypernym/similar synsets
        // This matches thesaurus behavior where "synonyms" includes close broader terms
        if is_synonym_only {
            if let Some(synset) = wn.get_synset(pos, id) {
                for &(rel_type, target_pos, target_id) in &synset.relations {
                    let include = matches!(
                        rel_type,
                        RelationType::Hypernym
                            | RelationType::InstanceHypernym
                            | RelationType::SimilarTo
                            | RelationType::AlsoSee
                            | RelationType::VerbGroup
                    );
                    if !include {
                        continue;
                    }
                    if let Some(target_synset) = wn.get_synset(target_pos, target_id) {
                        for ws in &target_synset.word_senses {
                            if ws.lemma.to_lowercase() == normalized {
                                continue;
                            }
                            let key =
                                format!("synonym:{}:{}", ws.lemma.to_lowercase(), target_id.0);
                            if seen.insert(key) {
                                results.push(json!({
                                    "value": ws.lemma.to_lowercase(),
                                    "relation": "synonym",
                                    "definition": target_synset.definition,
                                    "pos": target_pos.to_string(),
                                    "depth": 1,
                                }));
                            }
                        }
                    }
                }
            }
            continue;
        }

        // BFS for typed relations
        let mut frontier: Vec<(POS, SynsetId, u32)> = vec![(pos, id, 0)];
        let mut visited: HashSet<(u8, u32)> = HashSet::new();
        visited.insert((pos as u8, id.0));

        while let Some((cur_pos, cur_id, cur_depth)) = frontier.pop() {
            if cur_depth >= depth {
                continue;
            }
            let synset = match wn.get_synset(cur_pos, cur_id) {
                Some(s) => s,
                None => continue,
            };

            for &(rel_type, target_pos, target_id) in &synset.relations {
                // Filter by relation type if specified
                if let Some(ref types) = relation_types {
                    if !types.contains(&rel_type) {
                        continue;
                    }
                }

                if !visited.insert((target_pos as u8, target_id.0)) {
                    continue;
                }

                if let Some(target_synset) = wn.get_synset(target_pos, target_id) {
                    for ws in &target_synset.word_senses {
                        let key = format!(
                            "{}:{}:{}",
                            rel_type.name(),
                            ws.lemma.to_lowercase(),
                            target_id.0
                        );
                        if seen.insert(key) {
                            results.push(json!({
                                "value": ws.lemma.to_lowercase(),
                                "relation": rel_type.name(),
                                "definition": target_synset.definition,
                                "pos": target_pos.to_string(),
                                "depth": cur_depth + 1,
                            }));
                        }
                    }

                    // Continue BFS if more depth available
                    if cur_depth + 1 < depth {
                        frontier.push((target_pos, target_id, cur_depth + 1));
                    }
                }
            }
        }
    }

    Ok(results)
}

#[cfg(all(test, feature = "embedded-data"))]
mod tests {
    use super::*;
    use crate::args::RelatedArgs;

    fn make_args(
        word: &str,
        relation: Option<&str>,
        depth: Option<i64>,
        pos: Option<&str>,
        sense: Option<i64>,
    ) -> RelatedArgs {
        RelatedArgs {
            word: word.to_string(),
            relation: relation.map(|s| s.to_string()),
            depth,
            pos: pos.map(|s| s.to_string()),
            sense,
        }
    }

    #[test]
    fn test_synonyms() {
        let args = make_args("brave", Some("synonym"), None, None, None);
        let results = cmd_related(&args).unwrap();
        assert!(!results.is_empty(), "brave should have synonyms");
        for r in &results {
            assert_eq!(r["relation"], "synonym");
        }
    }

    #[test]
    fn test_antonyms() {
        let args = make_args("hot", Some("antonym"), None, None, None);
        let results = cmd_related(&args).unwrap();
        let has_cold = results.iter().any(|r| r["value"].as_str() == Some("cold"));
        assert!(has_cold, "hot should have cold as antonym");
    }

    #[test]
    fn test_hypernyms() {
        let args = make_args("dog", Some("hypernym"), None, Some("noun"), None);
        let results = cmd_related(&args).unwrap();
        assert!(!results.is_empty(), "dog should have hypernyms");
    }

    #[test]
    fn test_hyponyms() {
        let args = make_args("dog", Some("hyponym"), None, Some("noun"), None);
        let results = cmd_related(&args).unwrap();
        assert!(!results.is_empty(), "dog should have hyponyms");
    }

    #[test]
    fn test_derivations() {
        let args = make_args("brave", Some("derivation"), None, None, None);
        let results = cmd_related(&args).unwrap();
        let has_bravery = results
            .iter()
            .any(|r| r["value"].as_str() == Some("bravery"));
        assert!(has_bravery, "brave should derive bravery");
    }

    #[test]
    fn test_depth_control() {
        let d1 = cmd_related(&make_args("dog", Some("hypernym"), Some(1), Some("noun"), None)).unwrap();
        let d2 = cmd_related(&make_args("dog", Some("hypernym"), Some(2), Some("noun"), None)).unwrap();
        assert!(d2.len() >= d1.len(), "depth 2 should have at least as many results as depth 1");
    }

    #[test]
    fn test_sense_selection() {
        let all = cmd_related(&make_args("bank", None, None, None, None)).unwrap();
        let s1 = cmd_related(&make_args("bank", None, None, None, Some(1))).unwrap();
        assert!(s1.len() <= all.len(), "filtering by sense should reduce results");
    }

    #[test]
    fn test_pos_filter() {
        let args = make_args("run", None, None, Some("verb"), None);
        let results = cmd_related(&args).unwrap();
        assert!(!results.is_empty());
    }
}
