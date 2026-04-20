use std::collections::BTreeSet;
use std::sync::OnceLock;

use zr_word::models::{POS, RelationType};
use zr_word::wordnet::WordNet;

use super::data::{ROLE_SEEDS, SemanticSeed, TRAIT_SEEDS};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaggedTerm {
    pub value: String,
    pub tags: Vec<String>,
}

static ROLE_POOL: OnceLock<Vec<TaggedTerm>> = OnceLock::new();
static TRAIT_POOL: OnceLock<Vec<TaggedTerm>> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
pub struct RelationRecipe {
    pub include_hypernyms: bool,
    pub include_hyponyms: bool,
    pub include_similar: bool,
    pub include_also_see: bool,
    pub include_verb_group: bool,
}

impl RelationRecipe {
    fn includes(self, relation: RelationType) -> bool {
        match relation {
            RelationType::Hypernym | RelationType::InstanceHypernym => self.include_hypernyms,
            RelationType::Hyponym | RelationType::InstanceHyponym => self.include_hyponyms,
            RelationType::SimilarTo => self.include_similar,
            RelationType::AlsoSee => self.include_also_see,
            RelationType::VerbGroup => self.include_verb_group,
            _ => false,
        }
    }
}

pub const ARCHETYPE_RECIPE: RelationRecipe = RelationRecipe {
    include_hypernyms: false,
    include_hyponyms: true,
    include_similar: true,
    include_also_see: true,
    include_verb_group: false,
};

pub fn role_pool() -> &'static [TaggedTerm] {
    ROLE_POOL.get_or_init(|| build_pool(ROLE_SEEDS, POS::Noun, ARCHETYPE_RECIPE))
}

pub fn trait_pool() -> &'static [TaggedTerm] {
    TRAIT_POOL.get_or_init(|| build_pool(TRAIT_SEEDS, POS::Adj, ARCHETYPE_RECIPE))
}

fn build_pool(seeds: &[SemanticSeed], pos: POS, recipe: RelationRecipe) -> Vec<TaggedTerm> {
    let wn = WordNet::embedded();
    let mut terms = Vec::new();
    let mut seen = BTreeSet::new();

    for seed in seeds {
        let normalized = seed.seed.to_lowercase().replace(' ', "_");
        let mut added_seed = false;

        if let Some(senses) = wn.lookup_word(&normalized) {
            for &(sense_pos, synset_id) in senses {
                if sense_pos != pos {
                    continue;
                }

                if let Some(synset) = wn.get_synset(sense_pos, synset_id) {
                    collect_synset_terms(
                        &mut terms,
                        &mut seen,
                        synset.word_senses.iter().map(|ws| ws.lemma.as_str()),
                        seed.tags,
                    );

                    for &(relation, target_pos, target_id) in &synset.relations {
                        if !recipe.includes(relation) {
                            continue;
                        }
                        if target_pos != pos {
                            continue;
                        }
                        if let Some(target) = wn.get_synset(target_pos, target_id) {
                            collect_synset_terms(
                                &mut terms,
                                &mut seen,
                                target.word_senses.iter().map(|ws| ws.lemma.as_str()),
                                seed.tags,
                            );
                        }
                    }

                    added_seed = true;
                    break;
                }
            }
        }

        if !added_seed {
            push_term(&mut terms, &mut seen, seed.seed, seed.tags);
        }
    }

    terms.sort_by(|a, b| a.value.cmp(&b.value));
    terms
}

fn collect_synset_terms<'a>(
    terms: &mut Vec<TaggedTerm>,
    seen: &mut BTreeSet<String>,
    values: impl Iterator<Item = &'a str>,
    tags: &[&str],
) {
    for value in values {
        push_term(terms, seen, value, tags);
    }
}

fn push_term(terms: &mut Vec<TaggedTerm>, seen: &mut BTreeSet<String>, value: &str, tags: &[&str]) {
    let normalized = normalize_term(value);
    if normalized.is_empty() || !seen.insert(normalized.clone()) {
        return;
    }

    let mut term_tags: Vec<String> = tags.iter().map(|tag| (*tag).to_string()).collect();
    term_tags.sort();
    term_tags.dedup();

    terms.push(TaggedTerm {
        value: normalized,
        tags: term_tags,
    });
}

fn normalize_term(value: &str) -> String {
    let normalized = value.trim().to_lowercase().replace('_', " ");
    if normalized.is_empty() || normalized.contains('(') || normalized.contains(')') {
        return String::new();
    }
    normalized
}
