use std::collections::BTreeSet;

use rand::seq::SliceRandom;
use serde_json::{Map, Value};

use crate::args::CharacterArchetypeArgs;
use crate::make_rng;

use super::data::CURATED_LABELS;
use super::semantic::{TaggedTerm, role_pool, trait_pool};

#[derive(Clone, Debug, PartialEq, Eq)]
struct ArchetypeRecord {
    value: String,
    kind: String,
    tags: Vec<String>,
    role: Option<String>,
    label: Option<String>,
    traits: Vec<String>,
}

pub fn cmd_character_archetype(args: &CharacterArchetypeArgs) -> Result<Vec<Value>, String> {
    let count = args
        .count
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1)
        .max(1);
    let include = parse_tag_list(args.include.as_deref());
    let exclude = parse_tag_list(args.exclude.as_deref());

    let pool = build_candidate_pool(&include, &exclude);
    if pool.is_empty() {
        return Err("rand character archetype: no archetypes match the requested tags".to_string());
    }

    let mut rng = make_rng(args.seed);
    let mut results = Vec::with_capacity(count);
    for _ in 0..count {
        let record = pool
            .choose(&mut rng)
            .expect("candidate pool already checked as non-empty")
            .clone();
        results.push(record.into_json());
    }
    Ok(results)
}

fn build_candidate_pool(include: &[String], exclude: &[String]) -> Vec<ArchetypeRecord> {
    let mut pool = Vec::new();

    for role in role_pool() {
        push_if_match(&mut pool, include, exclude, role_only(role));
    }

    for label in CURATED_LABELS {
        push_if_match(
            &mut pool,
            include,
            exclude,
            label_only(label.label, label.tags),
        );
    }

    for trait_term in trait_pool() {
        for role in role_pool() {
            push_if_match(&mut pool, include, exclude, trait_role(trait_term, role));
        }
    }

    let traits = trait_pool();
    for first_idx in 0..traits.len() {
        for second_idx in (first_idx + 1)..traits.len() {
            let first = &traits[first_idx];
            let second = &traits[second_idx];
            for role in role_pool() {
                push_if_match(
                    &mut pool,
                    include,
                    exclude,
                    trait_pair_role(first, second, role),
                );
            }
        }
    }

    pool.sort_by(|a, b| a.value.cmp(&b.value).then(a.kind.cmp(&b.kind)));
    pool.dedup_by(|a, b| a.value == b.value && a.kind == b.kind);
    pool
}

fn push_if_match(
    pool: &mut Vec<ArchetypeRecord>,
    include: &[String],
    exclude: &[String],
    record: ArchetypeRecord,
) {
    if include
        .iter()
        .all(|tag| record.tags.iter().any(|own| own == tag))
        && exclude
            .iter()
            .all(|tag| record.tags.iter().all(|own| own != tag))
    {
        pool.push(record);
    }
}

fn role_only(role: &TaggedTerm) -> ArchetypeRecord {
    let role_text = title_case(&role.value);
    ArchetypeRecord {
        value: format!("{} {}", article_for(&role_text), role_text),
        kind: "role".to_string(),
        tags: merge_tags(&role.tags, &["archetype", "character"]),
        role: Some(role_text),
        label: None,
        traits: Vec::new(),
    }
}

fn label_only(label: &str, tags: &[&str]) -> ArchetypeRecord {
    let label_text = title_case(label);
    ArchetypeRecord {
        value: format!("{} {}", article_for(&label_text), label_text),
        kind: "label".to_string(),
        tags: merge_tags_slice(tags, &["archetype", "character"]),
        role: None,
        label: Some(label_text),
        traits: Vec::new(),
    }
}

fn trait_role(trait_term: &TaggedTerm, role: &TaggedTerm) -> ArchetypeRecord {
    let trait_text = title_case(&trait_term.value);
    let role_text = title_case(&role.value);
    let phrase = format!("{} {}", trait_text, role_text);
    ArchetypeRecord {
        value: format!("{} {}", article_for(&phrase), phrase),
        kind: "trait-role".to_string(),
        tags: merge_many_tags(&[&trait_term.tags, &role.tags], &["archetype", "character"]),
        role: Some(role_text),
        label: None,
        traits: vec![trait_text],
    }
}

fn trait_pair_role(first: &TaggedTerm, second: &TaggedTerm, role: &TaggedTerm) -> ArchetypeRecord {
    let first_text = title_case(&first.value);
    let second_text = title_case(&second.value);
    let role_text = title_case(&role.value);
    let phrase = format!("{}, {} {}", first_text, second_text, role_text);
    ArchetypeRecord {
        value: format!("{} {}", article_for(&phrase), phrase),
        kind: "trait-pair-role".to_string(),
        tags: merge_many_tags(
            &[&first.tags, &second.tags, &role.tags],
            &["archetype", "character"],
        ),
        role: Some(role_text),
        label: None,
        traits: vec![first_text, second_text],
    }
}

impl ArchetypeRecord {
    fn into_json(self) -> Value {
        let mut object = Map::new();
        object.insert("value".to_string(), Value::String(self.value));
        object.insert("kind".to_string(), Value::String(self.kind));
        object.insert(
            "tags".to_string(),
            Value::Array(self.tags.into_iter().map(Value::String).collect()),
        );
        if let Some(role) = self.role {
            object.insert("role".to_string(), Value::String(role));
        }
        if let Some(label) = self.label {
            object.insert("label".to_string(), Value::String(label));
        }
        if !self.traits.is_empty() {
            object.insert(
                "traits".to_string(),
                Value::Array(self.traits.into_iter().map(Value::String).collect()),
            );
        }
        Value::Object(object)
    }
}

fn parse_tag_list(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect()
}

fn merge_tags(tags: &[String], extra: &[&str]) -> Vec<String> {
    let mut merged = BTreeSet::new();
    for tag in tags {
        merged.insert(tag.clone());
    }
    for tag in extra {
        merged.insert((*tag).to_string());
    }
    merged.into_iter().collect()
}

fn merge_tags_slice(tags: &[&str], extra: &[&str]) -> Vec<String> {
    let mut merged = BTreeSet::new();
    for tag in tags {
        merged.insert((*tag).to_string());
    }
    for tag in extra {
        merged.insert((*tag).to_string());
    }
    merged.into_iter().collect()
}

fn merge_many_tags(tag_sets: &[&[String]], extra: &[&str]) -> Vec<String> {
    let mut merged = BTreeSet::new();
    for tag_set in tag_sets {
        for tag in *tag_set {
            merged.insert(tag.clone());
        }
    }
    for tag in extra {
        merged.insert((*tag).to_string());
    }
    merged.into_iter().collect()
}

fn article_for(value: &str) -> &'static str {
    match value.chars().next().map(|ch| ch.to_ascii_lowercase()) {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "An",
        _ => "A",
    }
}

fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::WordArgs;
    use crate::cmd_word;
    use serde_json::json;
    use std::collections::BTreeMap;
    use zacor_package::FromArgs;

    fn make_args(pairs: &[(&str, Value)]) -> CharacterArchetypeArgs {
        let map: BTreeMap<String, Value> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        CharacterArchetypeArgs::from_args(&map).unwrap()
    }

    fn make_word_args(pairs: &[(&str, Value)]) -> WordArgs {
        let map: BTreeMap<String, Value> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        WordArgs::from_args(&map).unwrap()
    }

    #[test]
    fn generates_one_record_by_default() {
        let result = cmd_character_archetype(&make_args(&[("seed", json!(42))])).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn count_generates_multiple_records() {
        let result =
            cmd_character_archetype(&make_args(&[("count", json!(3)), ("seed", json!(42))]))
                .unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn seed_is_deterministic() {
        let a = cmd_character_archetype(&make_args(&[("count", json!(4)), ("seed", json!(7))]))
            .unwrap();
        let b = cmd_character_archetype(&make_args(&[("count", json!(4)), ("seed", json!(7))]))
            .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn includes_structured_fields() {
        let result = cmd_character_archetype(&make_args(&[
            ("include", json!("romance")),
            ("seed", json!(42)),
        ]))
        .unwrap();
        let record = &result[0];
        assert!(record["value"].as_str().is_some());
        assert!(record["kind"].as_str().is_some());
        assert!(record["tags"].as_array().is_some());
        assert!(record.get("role").is_some() || record.get("label").is_some());
    }

    #[test]
    fn strict_include_filter_is_applied() {
        let result = cmd_character_archetype(&make_args(&[
            ("include", json!("romance,outcast")),
            ("count", json!(5)),
            ("seed", json!(42)),
        ]))
        .unwrap();

        for record in result {
            let tags = record["tags"].as_array().unwrap();
            assert!(tags.iter().any(|tag| tag.as_str() == Some("romance")));
            assert!(tags.iter().any(|tag| tag.as_str() == Some("outcast")));
        }
    }

    #[test]
    fn strict_exclude_filter_is_applied() {
        let result = cmd_character_archetype(&make_args(&[
            ("exclude", json!("supernatural")),
            ("count", json!(5)),
            ("seed", json!(42)),
        ]))
        .unwrap();

        for record in result {
            let tags = record["tags"].as_array().unwrap();
            assert!(tags.iter().all(|tag| tag.as_str() != Some("supernatural")));
        }
    }

    #[test]
    fn template_kinds_are_available() {
        let pool = build_candidate_pool(&[], &[]);
        assert!(pool.iter().any(|record| record.kind == "role"));
        assert!(pool.iter().any(|record| record.kind == "label"));
        assert!(pool.iter().any(|record| record.kind == "trait-role"));
        assert!(pool.iter().any(|record| record.kind == "trait-pair-role"));
    }

    #[test]
    fn rand_word_behavior_remains_unchanged() {
        let word_args = make_word_args(&[("count", json!(3)), ("seed", json!(42))]);
        let before = cmd_word(&word_args).unwrap();
        let _ = cmd_character_archetype(&make_args(&[("count", json!(5)), ("seed", json!(42))]))
            .unwrap();
        let after = cmd_word(&word_args).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn package_metadata_contains_nested_archetype_command() {
        let package_yaml = include_str!("../../package.yaml");
        assert!(package_yaml.contains("  character:\n"));
        assert!(package_yaml.contains("      archetype:\n"));
    }
}
