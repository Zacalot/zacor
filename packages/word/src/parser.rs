use crate::models::*;
use std::collections::HashMap;

/// Parse a WordNet data file (data.adj, data.noun, etc.) into synsets.
/// Applies frequency data from cntlist if provided.
pub fn parse_data_file(text: &str, freq_map: &HashMap<String, u32>) -> Vec<Synset> {
    let mut synsets = Vec::new();
    for line in text.lines() {
        // Skip comment lines (start with two spaces)
        if line.starts_with("  ") {
            continue;
        }
        if let Some(synset) = parse_data_line(line, freq_map) {
            synsets.push(synset);
        }
    }
    synsets
}

/// Parse a single data line into a Synset.
fn parse_data_line(line: &str, freq_map: &HashMap<String, u32>) -> Option<Synset> {
    // Split at pipe to separate data from gloss
    let (data_part, gloss_part) = line.split_once(" | ")?;
    let tokens: Vec<&str> = data_part.split_whitespace().collect();
    if tokens.len() < 6 {
        return None;
    }

    let offset: u32 = tokens[0].parse().ok()?;
    let lex_filenum: u8 = tokens[1].parse().ok()?;
    let ss_type = tokens[2].chars().next()?;
    let pos = POS::from_ss_type(ss_type)?;
    let w_cnt = u32::from_str_radix(tokens[3], 16).ok()?;

    // Parse words: pairs of (word, lex_id)
    let mut word_senses = Vec::with_capacity(w_cnt as usize);
    let mut idx = 4;
    for _ in 0..w_cnt {
        if idx + 1 >= tokens.len() {
            break;
        }
        let lemma = tokens[idx].to_string();
        // lex_id is hex single digit
        let _lex_id = u32::from_str_radix(tokens[idx + 1], 16).unwrap_or(0);
        // Look up frequency from freq_map using the sense key pattern
        let freq_key = format!("{}:{}", lemma.to_lowercase(), offset);
        let frequency = freq_map.get(&freq_key).copied().unwrap_or(0);
        word_senses.push(WordSense { lemma, frequency });
        idx += 2;
    }

    // Parse pointer count and pointers
    if idx >= tokens.len() {
        return None;
    }
    let p_cnt: u32 = tokens[idx].parse().ok()?;
    idx += 1;

    let mut relations = Vec::new();
    for _ in 0..p_cnt {
        if idx + 3 >= tokens.len() {
            break;
        }
        let symbol = tokens[idx];
        let target_offset: u32 = tokens[idx + 1].parse().unwrap_or(0);
        let target_pos_char = tokens[idx + 2].chars().next().unwrap_or('n');
        let _source_target = tokens[idx + 3];
        idx += 4;

        if let (Some(rel_type), Some(target_pos)) = (
            RelationType::from_pointer_symbol(symbol),
            POS::from_ss_type(target_pos_char),
        ) {
            relations.push((rel_type, target_pos, SynsetId(target_offset)));
        }
    }

    // Parse gloss: definition + examples
    let (definition, examples) = parse_gloss(gloss_part);

    Some(Synset {
        id: SynsetId(offset),
        pos,
        domain: lex_filenum,
        word_senses,
        definition,
        examples,
        relations,
    })
}

/// Parse gloss text into definition and examples.
/// Gloss format: "definition text; \"example 1\"; \"example 2\""
fn parse_gloss(gloss: &str) -> (String, Vec<String>) {
    let gloss = gloss.trim();
    let mut examples = Vec::new();

    // Find first quoted example
    let definition;
    if let Some(quote_pos) = gloss.find('"') {
        definition = gloss[..quote_pos].trim().trim_end_matches(';').trim().to_string();
        // Extract all quoted strings
        let mut chars = gloss[quote_pos..].chars().peekable();
        while let Some(&c) = chars.peek() {
            if c == '"' {
                chars.next(); // consume opening quote
                let mut example = String::new();
                for ch in chars.by_ref() {
                    if ch == '"' {
                        break;
                    }
                    example.push(ch);
                }
                if !example.is_empty() {
                    examples.push(example);
                }
            } else {
                chars.next();
            }
        }
    } else {
        definition = gloss.to_string();
    }

    (definition, examples)
}

/// Parse index.sense file into word → Vec<(POS, SynsetId)> mapping.
/// Format: sense_key synset_offset sense_number tag_cnt
/// sense_key: lemma%ss_type:lex_filenum:lex_id:head_word:head_id
pub fn parse_index_sense(text: &str) -> Vec<(String, Vec<(POS, SynsetId)>)> {
    let mut map: HashMap<String, Vec<(POS, SynsetId)>> = HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let sense_key = parts[0];
        let synset_offset: u32 = match parts[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Parse sense key: lemma%ss_type:lex_filenum:lex_id:head_word:head_id
        if let Some(pct_pos) = sense_key.find('%') {
            let lemma = &sense_key[..pct_pos];
            let rest = &sense_key[pct_pos + 1..];
            let type_parts: Vec<&str> = rest.split(':').collect();
            if type_parts.is_empty() {
                continue;
            }
            let ss_type: u8 = match type_parts[0].parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            if let Some(pos) = POS::from_sense_type(ss_type) {
                let normalized = lemma.to_lowercase();
                map.entry(normalized).or_default().push((pos, SynsetId(synset_offset)));
            }
        }
    }

    // Convert to sorted vec for binary search
    let mut index: Vec<(String, Vec<(POS, SynsetId)>)> = map.into_iter().collect();
    index.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    // Deduplicate within each entry
    for (_, senses) in &mut index {
        senses.sort_unstable_by_key(|&(pos, id)| (pos as u8, id.0));
        senses.dedup();
    }
    index
}

/// Parse cntlist file into frequency map.
/// Format: count sense_key sense_number
/// Returns map of "lemma:synset_offset" → count
pub fn parse_cntlist(text: &str) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let count: u32 = match parts[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let sense_key = parts[1];

        // Parse sense_key to extract lemma and find synset offset
        // We need the synset offset from index.sense, but cntlist doesn't have it directly.
        // Store by sense_key for now, we'll resolve during data loading.
        map.insert(sense_key.to_string(), count);
    }
    map
}

/// Build a frequency lookup map from cntlist + index.sense data.
/// Returns map of "lowercase_lemma:synset_offset" → frequency count.
pub fn build_freq_map(cntlist_text: &str, index_sense_text: &str) -> HashMap<String, u32> {
    let mut result = HashMap::new();

    // First, build sense_key → synset_offset from index.sense
    let mut sense_to_offset: HashMap<String, u32> = HashMap::new();
    for line in index_sense_text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let offset: u32 = parts[1].parse().unwrap_or(0);
            sense_to_offset.insert(parts[0].to_string(), offset);
        }
    }

    // Now parse cntlist and resolve to "lemma:offset" keys
    for line in cntlist_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let count: u32 = match parts[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let sense_key = parts[1];

        // Get the synset offset for this sense_key
        if let Some(&offset) = sense_to_offset.get(sense_key) {
            // Extract lemma from sense_key
            if let Some(pct_pos) = sense_key.find('%') {
                let lemma = sense_key[..pct_pos].to_lowercase();
                let key = format!("{}:{}", lemma, offset);
                result.insert(key, count);
            }
        }
    }

    result
}

/// Lookup a word in the sorted index via binary search.
pub fn index_lookup<'a>(
    index: &'a [(String, Vec<(POS, SynsetId)>)],
    word: &str,
) -> Option<&'a Vec<(POS, SynsetId)>> {
    let normalized = word.to_lowercase().replace(' ', "_");
    index
        .binary_search_by(|(w, _)| w.as_str().cmp(&normalized))
        .ok()
        .map(|i| &index[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gloss_definition_only() {
        let (def, examples) = parse_gloss("a brave person");
        assert_eq!(def, "a brave person");
        assert!(examples.is_empty());
    }

    #[test]
    fn test_parse_gloss_with_examples() {
        let (def, examples) =
            parse_gloss(r#"a brave person; "he was brave"; "she is courageous""#);
        assert_eq!(def, "a brave person");
        assert_eq!(examples.len(), 2);
        assert_eq!(examples[0], "he was brave");
        assert_eq!(examples[1], "she is courageous");
    }

    #[test]
    fn test_parse_data_line() {
        let line = r#"00001740 00 a 01 able 0 005 = 05207437 n 0000 = 05624029 n 0000 + 05624029 n 0101 + 05207437 n 0101 ! 00002098 a 0101 | (usually followed by `to') having the necessary means or skill or know-how or authority to do something; "able to swim"; "she was able to program her computer""#;
        let freq_map = HashMap::new();
        let synset = parse_data_line(line, &freq_map).unwrap();
        assert_eq!(synset.id, SynsetId(1740));
        assert_eq!(synset.pos, POS::Adj);
        assert_eq!(synset.domain, 0);
        assert_eq!(synset.word_senses.len(), 1);
        assert_eq!(synset.word_senses[0].lemma, "able");
        assert_eq!(synset.relations.len(), 5);
        assert!(!synset.definition.is_empty());
        assert_eq!(synset.examples.len(), 2);
    }

    #[test]
    fn test_parse_data_line_multi_word() {
        let line = r#"00002312 00 a 02 abaxial 0 dorsal 4 002 ;c 06047178 n 0000 ! 00002527 a 0101 | facing away from the axis"#;
        let freq_map = HashMap::new();
        let synset = parse_data_line(line, &freq_map).unwrap();
        assert_eq!(synset.word_senses.len(), 2);
        assert_eq!(synset.word_senses[0].lemma, "abaxial");
        assert_eq!(synset.word_senses[1].lemma, "dorsal");
    }

    #[test]
    fn test_index_lookup() {
        let index = vec![
            ("able".to_string(), vec![(POS::Adj, SynsetId(1740))]),
            ("bank".to_string(), vec![(POS::Noun, SynsetId(100)), (POS::Verb, SynsetId(200))]),
            ("dog".to_string(), vec![(POS::Noun, SynsetId(300))]),
        ];
        assert!(index_lookup(&index, "bank").is_some());
        assert!(index_lookup(&index, "Bank").is_some()); // case insensitive
        assert!(index_lookup(&index, "xyz").is_none());
    }
}
