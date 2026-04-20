use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rand::Rng;
use serde_json::{Map, Value};
use zr_word::models::{POS, RelationType, Synset, WordSense};
use zr_word::wordnet::WordNet;

use crate::args::CharacterMotiveArgs;
use crate::make_rng;

use super::data::SemanticSeed;
use super::motive_data::{
    DRIVE_SEEDS, FORCE_TENSION_SEEDS, GOAL_ACTION_SEEDS, GOAL_OBJECT_SEEDS, NARRATIVE_TEMPLATES,
    OBSTACLE_TEMPLATES, OUTCOME_TEMPLATES,
};

const NOUN_COGNITION: u8 = 9;
const NOUN_FEELING: u8 = 12;
const NOUN_GROUP: u8 = 14;
const NOUN_MOTIVE: u8 = 16;
const NOUN_POSSESSION: u8 = 21;
const NOUN_RELATION: u8 = 24;
const NOUN_STATE: u8 = 26;
const NOUN_ATTRIBUTE: u8 = 7;

const VERB_CHANGE: u8 = 30;
const VERB_COGNITION: u8 = 31;
const VERB_COMPETITION: u8 = 33;
const VERB_EMOTION: u8 = 37;
const VERB_MOTION: u8 = 38;
const VERB_POSSESSION: u8 = 40;
const VERB_SOCIAL: u8 = 41;

const DRIVE_DOMAINS: &[u8] = &[NOUN_MOTIVE, NOUN_FEELING, NOUN_STATE, NOUN_ATTRIBUTE];
const ACTION_DOMAINS: &[u8] = &[
    VERB_SOCIAL,
    VERB_COGNITION,
    VERB_CHANGE,
    VERB_POSSESSION,
    VERB_EMOTION,
    VERB_COMPETITION,
    VERB_MOTION,
];
const OBJECT_DOMAINS: &[u8] = &[
    NOUN_MOTIVE,
    NOUN_COGNITION,
    NOUN_GROUP,
    NOUN_POSSESSION,
    NOUN_RELATION,
    NOUN_STATE,
    NOUN_ATTRIBUTE,
];
const TENSION_DOMAINS: &[u8] = &[NOUN_MOTIVE, NOUN_FEELING, NOUN_STATE, NOUN_ATTRIBUTE];

const NOUN_RELATIONS: &[RelationType] = &[
    RelationType::Hypernym,
    RelationType::InstanceHypernym,
    RelationType::Hyponym,
    RelationType::InstanceHyponym,
    RelationType::SimilarTo,
    RelationType::AlsoSee,
    RelationType::Attribute,
];

const VERB_RELATIONS: &[RelationType] = &[
    RelationType::VerbGroup,
    RelationType::AlsoSee,
    RelationType::Entailment,
    RelationType::Cause,
];

const MAX_DEPTH: u8 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CandidateTerm {
    value: String,
    tags: Vec<String>,
    domain: u8,
    distance: u8,
    weight: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GoalCandidate {
    value: String,
    tags: Vec<String>,
    action: String,
    object: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClauseCandidate {
    value: String,
    tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MotiveRecord {
    value: String,
    kind: String,
    tags: Vec<String>,
    drive: Option<String>,
    goal: Option<String>,
    obstacle: Option<String>,
    outcome: Option<String>,
    forces: Vec<String>,
}

pub fn cmd_character_motive(args: &CharacterMotiveArgs) -> Result<Vec<Value>, String> {
    let count = args
        .count
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1)
        .max(1);
    let include = parse_tag_list(args.include.as_deref());
    let exclude = parse_tag_list(args.exclude.as_deref());

    let mut rng = make_rng(args.seed);
    let mut results = Vec::with_capacity(count);
    let mut seen_values = BTreeSet::new();

    for _ in 0..(count * 40) {
        if results.len() >= count {
            break;
        }

        let record = generate_record(&mut rng)?;
        if !record_matches_filters(&record, &include, &exclude) {
            continue;
        }
        if !seen_values.insert(record.value.clone()) {
            continue;
        }
        results.push(record.into_json());
    }

    if results.is_empty() {
        return Err("rand character motive: no motives match the requested tags".to_string());
    }

    Ok(results)
}

fn generate_record(rng: &mut impl Rng) -> Result<MotiveRecord, String> {
    for _ in 0..60 {
        let drive_seed = pick_seed(DRIVE_SEEDS, None, rng)?;
        let drive = sample_for_seed(drive_seed, POS::Noun, DRIVE_DOMAINS, rng, is_usable_drive)?;

        if rng.gen_bool(0.18) {
            let tension_seed = pick_seed(FORCE_TENSION_SEEDS, Some(&drive.tags), rng)?;
            let tension = sample_for_seed(
                tension_seed,
                POS::Noun,
                TENSION_DOMAINS,
                rng,
                is_usable_tension,
            )?;
            if drive.value == tension.value {
                continue;
            }
            let tags = merge_dynamic_tags(
                &[drive.tags.as_slice(), tension.tags.as_slice()],
                &["character", "motive"],
            );
            return Ok(MotiveRecord {
                value: format!(
                    "{} versus {}.",
                    title_case(&drive.value),
                    sentence_case(&tension.value)
                ),
                kind: "forces".to_string(),
                tags,
                drive: None,
                goal: None,
                obstacle: None,
                outcome: None,
                forces: vec![title_case(&drive.value), sentence_case(&tension.value)],
            });
        }

        let action_seed = pick_seed(GOAL_ACTION_SEEDS, Some(&drive.tags), rng)?;
        let object_seed = pick_seed(GOAL_OBJECT_SEEDS, Some(&drive.tags), rng)?;

        let action = sample_for_seed(
            action_seed,
            POS::Verb,
            ACTION_DOMAINS,
            rng,
            is_usable_action,
        )?;
        let object = sample_for_seed(
            object_seed,
            POS::Noun,
            OBJECT_DOMAINS,
            rng,
            is_usable_object,
        )?;
        if !is_goal_pair_compatible(&action, &object) {
            continue;
        }

        let goal = GoalCandidate {
            value: format!(
                "{} {}",
                sentence_case(&action.value),
                format_object_phrase(&object)
            ),
            tags: merge_dynamic_tags(&[action.tags.as_slice(), object.tags.as_slice()], &[]),
            action: action.value.clone(),
            object: object.value.clone(),
        };

        let mut obstacle_clause = None;
        if rng.gen_bool(0.52) {
            let tension_seed = pick_seed(FORCE_TENSION_SEEDS, Some(&drive.tags), rng)?;
            let alt_action_seed = pick_seed(GOAL_ACTION_SEEDS, Some(&drive.tags), rng)?;
            let alt_object_seed = pick_seed(GOAL_OBJECT_SEEDS, Some(&drive.tags), rng)?;
            let tension = sample_for_seed(
                tension_seed,
                POS::Noun,
                TENSION_DOMAINS,
                rng,
                is_usable_tension,
            )?;
            let alt_action = sample_for_seed(
                alt_action_seed,
                POS::Verb,
                ACTION_DOMAINS,
                rng,
                is_usable_action,
            )?;
            let alt_object = sample_for_seed(
                alt_object_seed,
                POS::Noun,
                OBJECT_DOMAINS,
                rng,
                is_usable_object,
            )?;
            if is_clause_compatible(&tension, &alt_action, &alt_object, true) {
                obstacle_clause = Some(ClauseCandidate {
                    value: render_clause(
                        sample_template(OBSTACLE_TEMPLATES, rng),
                        &tension,
                        &alt_action,
                        &alt_object,
                    ),
                    tags: merge_dynamic_tags(
                        &[
                            tension.tags.as_slice(),
                            alt_action.tags.as_slice(),
                            alt_object.tags.as_slice(),
                        ],
                        &[],
                    ),
                });
            }
        }

        let mut outcome_clause = None;
        if rng.gen_bool(0.48) {
            let out_action_seed = pick_seed(GOAL_ACTION_SEEDS, Some(&drive.tags), rng)?;
            let out_object_seed = pick_seed(GOAL_OBJECT_SEEDS, Some(&drive.tags), rng)?;
            let out_action = sample_for_seed(
                out_action_seed,
                POS::Verb,
                ACTION_DOMAINS,
                rng,
                is_usable_action,
            )?;
            let out_object = sample_for_seed(
                out_object_seed,
                POS::Noun,
                OBJECT_DOMAINS,
                rng,
                is_usable_object,
            )?;
            if is_clause_compatible(&drive, &out_action, &out_object, false) {
                outcome_clause = Some(ClauseCandidate {
                    value: render_outcome_clause(
                        sample_template(OUTCOME_TEMPLATES, rng),
                        &out_action,
                        &out_object,
                    ),
                    tags: merge_dynamic_tags(
                        &[out_action.tags.as_slice(), out_object.tags.as_slice()],
                        &[],
                    ),
                });
            }
        }

        let tags = merge_dynamic_tags(
            &[
                drive.tags.as_slice(),
                goal.tags.as_slice(),
                obstacle_clause
                    .as_ref()
                    .map(|clause| clause.tags.as_slice())
                    .unwrap_or(&[]),
                outcome_clause
                    .as_ref()
                    .map(|clause| clause.tags.as_slice())
                    .unwrap_or(&[]),
            ],
            &["character", "motive"],
        );

        let value = render_narrative(
            &drive,
            &goal,
            obstacle_clause.as_ref(),
            outcome_clause.as_ref(),
        );

        if looks_degenerate(
            &drive.value,
            &goal,
            obstacle_clause.as_ref(),
            outcome_clause.as_ref(),
        ) {
            continue;
        }

        return Ok(MotiveRecord {
            value,
            kind: "narrative".to_string(),
            tags,
            drive: Some(title_case(&drive.value)),
            goal: Some(goal.value),
            obstacle: obstacle_clause.map(|clause| sentence_case(&clause.value)),
            outcome: outcome_clause.map(|clause| sentence_case(&clause.value)),
            forces: Vec::new(),
        });
    }

    Err("rand character motive: failed to build a coherent motive".to_string())
}

fn pick_seed<'a>(
    seeds: &'a [SemanticSeed],
    preferred_tags: Option<&[String]>,
    rng: &mut impl Rng,
) -> Result<&'a SemanticSeed, String> {
    let filtered: Vec<_> = seeds
        .iter()
        .filter(|seed| {
            preferred_tags
                .map(|tags| {
                    tags.iter()
                        .any(|tag| seed.tags.iter().any(|seed_tag| tag == seed_tag))
                })
                .unwrap_or(true)
        })
        .collect();
    let pool = if filtered.is_empty() {
        seeds.iter().collect()
    } else {
        filtered
    };
    pool.get(rng.gen_range(0..pool.len()))
        .copied()
        .ok_or_else(|| "rand character motive: no seed candidates available".to_string())
}

fn sample_for_seed(
    seed: &SemanticSeed,
    pos: POS,
    allowed_domains: &[u8],
    rng: &mut impl Rng,
    is_usable: fn(&str) -> bool,
) -> Result<CandidateTerm, String> {
    let candidates = collect_candidates(seed, pos, allowed_domains, is_usable);
    weighted_choice(&candidates, rng)
        .cloned()
        .ok_or_else(|| "rand character motive: empty candidate pool".to_string())
}

fn collect_candidates(
    seed: &SemanticSeed,
    pos: POS,
    allowed_domains: &[u8],
    is_usable: fn(&str) -> bool,
) -> Vec<CandidateTerm> {
    let wn = WordNet::embedded();
    let mut pool = BTreeMap::new();
    let mut frontier = VecDeque::new();
    let mut seen_synsets = BTreeSet::new();
    let normalized_seed = normalize_lookup(seed.seed);
    let relation_types = relation_types_for_pos(pos);

    if let Some(senses) = wn.lookup_word(&normalized_seed) {
        for &(sense_pos, synset_id) in senses {
            if sense_pos != pos {
                continue;
            }
            let Some(synset) = wn.get_synset(sense_pos, synset_id) else {
                continue;
            };
            if !allowed_domains.contains(&synset.domain) {
                continue;
            }
            if seen_synsets.insert((pos_key(sense_pos), synset_id.0)) {
                frontier.push_back((sense_pos, synset_id, 0u8));
            }
        }
    }

    while let Some((current_pos, current_id, distance)) = frontier.pop_front() {
        let Some(synset) = wn.get_synset(current_pos, current_id) else {
            continue;
        };
        collect_synset_terms(&mut pool, synset, seed, is_usable, distance);
        if distance >= MAX_DEPTH {
            continue;
        }

        let mut relations: Vec<_> = synset
            .relations
            .iter()
            .filter(|(relation, target_pos, _)| {
                *target_pos == pos && relation_types.contains(relation)
            })
            .copied()
            .collect();
        relations.sort_by_key(|(_, _, target_id)| target_id.0);

        for (_, target_pos, target_id) in relations {
            let Some(target) = wn.get_synset(target_pos, target_id) else {
                continue;
            };
            if !allowed_domains.contains(&target.domain) {
                continue;
            }
            if seen_synsets.insert((pos_key(target_pos), target_id.0)) {
                frontier.push_back((target_pos, target_id, distance + 1));
            }
        }
    }

    let mut values: Vec<_> = pool.into_values().collect();
    values.sort_by(|a, b| b.weight.cmp(&a.weight).then(a.value.cmp(&b.value)));
    if values.is_empty() {
        let fallback = seed.seed.trim().to_lowercase();
        if is_usable(&fallback) && !looks_noisy(&fallback) {
            values.push(CandidateTerm {
                value: fallback,
                tags: seed.tags.iter().map(|tag| (*tag).to_string()).collect(),
                domain: *allowed_domains.first().unwrap_or(&0),
                distance: 0,
                weight: 40,
            });
        }
    }
    values.truncate(32);
    values
}

fn collect_synset_terms(
    pool: &mut BTreeMap<String, CandidateTerm>,
    synset: &Synset,
    seed: &SemanticSeed,
    is_usable: fn(&str) -> bool,
    distance: u8,
) {
    let canonical = seed.seed.trim().to_lowercase();

    for word_sense in &synset.word_senses {
        let value = normalize_lemma(&word_sense.lemma);
        if value.is_empty() || !is_usable(&value) || looks_noisy(&value) {
            continue;
        }

        let weight = score_candidate(&value, word_sense, synset.domain, &canonical, distance);
        if weight == 0 {
            continue;
        }

        let candidate = CandidateTerm {
            value: value.clone(),
            tags: seed.tags.iter().map(|tag| (*tag).to_string()).collect(),
            domain: synset.domain,
            distance,
            weight,
        };

        match pool.get_mut(&value) {
            Some(existing) => {
                if candidate.weight > existing.weight {
                    *existing = candidate;
                } else {
                    merge_tag_slice(&mut existing.tags, seed.tags);
                    existing.distance = existing.distance.min(distance);
                }
            }
            None => {
                pool.insert(value, candidate);
            }
        }
    }
}

fn weighted_choice<'a, T>(items: &'a [T], rng: &mut impl Rng) -> Option<&'a T>
where
    T: Weighted,
{
    let total: u32 = items.iter().map(Weighted::weight).sum();
    if total == 0 || items.is_empty() {
        return None;
    }
    let mut ticket = rng.gen_range(0..total);
    for item in items {
        if ticket < item.weight() {
            return Some(item);
        }
        ticket -= item.weight();
    }
    items.last()
}

trait Weighted {
    fn weight(&self) -> u32;
}

impl Weighted for CandidateTerm {
    fn weight(&self) -> u32 {
        self.weight
    }
}

fn score_candidate(
    value: &str,
    word_sense: &WordSense,
    domain: u8,
    canonical: &str,
    distance: u8,
) -> u32 {
    let mut score = domain_weight(domain) + (word_sense.frequency.min(12) * 7);
    score += distance_weight(distance);
    if value == canonical {
        score = score.saturating_sub(65);
    }

    let word_count = value.split_whitespace().count() as u32;
    score = score.saturating_sub(word_count.saturating_sub(1) * 14);
    if value.len() > 12 {
        score = score.saturating_sub(((value.len() - 12) as u32) * 2);
    }
    if word_sense.frequency == 0 {
        score = score.saturating_sub(30);
    }
    score
}

fn domain_weight(domain: u8) -> u32 {
    match domain {
        NOUN_MOTIVE => 120,
        NOUN_FEELING => 108,
        NOUN_STATE => 102,
        NOUN_ATTRIBUTE => 92,
        NOUN_COGNITION => 88,
        NOUN_RELATION => 84,
        NOUN_POSSESSION => 76,
        NOUN_GROUP => 70,
        VERB_SOCIAL => 108,
        VERB_COGNITION => 100,
        VERB_CHANGE => 94,
        VERB_POSSESSION => 88,
        VERB_EMOTION => 84,
        VERB_COMPETITION => 80,
        VERB_MOTION => 70,
        _ => 40,
    }
}

fn distance_weight(distance: u8) -> u32 {
    match distance {
        0 => 6,
        1 => 48,
        2 => 34,
        3 => 18,
        _ => 0,
    }
}

fn relation_types_for_pos(pos: POS) -> &'static [RelationType] {
    match pos {
        POS::Noun => NOUN_RELATIONS,
        POS::Verb => VERB_RELATIONS,
        _ => &[],
    }
}

fn pos_key(pos: POS) -> u8 {
    match pos {
        POS::Adj => 0,
        POS::Noun => 1,
        POS::Verb => 2,
        POS::Adv => 3,
    }
}

fn is_goal_pair_compatible(action: &CandidateTerm, object: &CandidateTerm) -> bool {
    if !shares_any_tags(&action.tags, &object.tags) {
        return false;
    }

    match action.domain {
        VERB_COGNITION => matches!(
            object.domain,
            NOUN_COGNITION | NOUN_MOTIVE | NOUN_ATTRIBUTE | NOUN_STATE
        ),
        VERB_SOCIAL => matches!(
            object.domain,
            NOUN_GROUP | NOUN_RELATION | NOUN_POSSESSION | NOUN_STATE
        ),
        VERB_POSSESSION => matches!(object.domain, NOUN_POSSESSION | NOUN_RELATION | NOUN_STATE),
        VERB_CHANGE => matches!(object.domain, NOUN_STATE | NOUN_RELATION | NOUN_ATTRIBUTE),
        VERB_EMOTION => matches!(object.domain, NOUN_RELATION | NOUN_GROUP | NOUN_STATE),
        VERB_COMPETITION => matches!(object.domain, NOUN_POSSESSION | NOUN_GROUP | NOUN_STATE),
        VERB_MOTION => matches!(object.domain, NOUN_STATE | NOUN_RELATION),
        _ => true,
    }
}

fn is_clause_compatible(
    tension: &CandidateTerm,
    action: &CandidateTerm,
    object: &CandidateTerm,
    is_obstacle: bool,
) -> bool {
    let action_ok = is_goal_pair_compatible(action, object);
    let tension_ok = shares_any_tags(&tension.tags, &object.tags)
        || matches!(object.domain, NOUN_STATE | NOUN_RELATION | NOUN_ATTRIBUTE);
    if is_obstacle {
        action_ok && tension_ok
    } else {
        action_ok
    }
}

fn render_narrative(
    drive: &CandidateTerm,
    goal: &GoalCandidate,
    obstacle: Option<&ClauseCandidate>,
    outcome: Option<&ClauseCandidate>,
) -> String {
    let drive_text = title_case(&drive.value);
    let template =
        &NARRATIVE_TEMPLATES[(drive.value.len() + goal.value.len()) % NARRATIVE_TEMPLATES.len()];

    let base = match (obstacle, outcome) {
        (Some(_obstacle), Some(_outcome)) => template.with_both,
        (Some(_), None) => template.with_obstacle,
        (None, Some(_)) => template.with_outcome,
        (None, None) => template.base,
    };

    base.replace("{drive}", &drive_text)
        .replace("{goal}", &goal.value)
        .replace(
            "{obstacle}",
            obstacle
                .map(|c| sentence_case(&c.value))
                .as_deref()
                .unwrap_or(""),
        )
        .replace(
            "{outcome}",
            outcome
                .map(|c| sentence_case(&c.value))
                .as_deref()
                .unwrap_or(""),
        )
}

fn render_clause(
    template: &str,
    tension: &CandidateTerm,
    action: &CandidateTerm,
    object: &CandidateTerm,
) -> String {
    template
        .replace("{tension}", &sentence_case(&tension.value))
        .replace("{action}", &sentence_case(&action.value))
        .replace("{object}", &format_object_phrase(object))
        .replace("{stake}", &format_stake_phrase(object))
}

fn render_outcome_clause(template: &str, action: &CandidateTerm, object: &CandidateTerm) -> String {
    template
        .replace("{action}", &sentence_case(&action.value))
        .replace("{object}", &format_object_phrase(object))
}

fn sample_template<'a>(templates: &'a [&'a str], rng: &mut impl Rng) -> &'a str {
    templates[rng.gen_range(0..templates.len())]
}

fn looks_degenerate(
    drive: &str,
    goal: &GoalCandidate,
    obstacle: Option<&ClauseCandidate>,
    outcome: Option<&ClauseCandidate>,
) -> bool {
    let drive_l = drive.to_lowercase();
    let goal_l = goal.value.to_lowercase();
    if goal_l.contains(&drive_l) {
        return true;
    }
    if let Some(obstacle) = obstacle {
        let obstacle_l = obstacle.value.to_lowercase();
        if obstacle_l.contains(&goal_l) {
            return true;
        }
    }
    if let Some(outcome) = outcome {
        let outcome_l = outcome.value.to_lowercase();
        if outcome_l == goal_l {
            return true;
        }
    }
    false
}

fn format_object_phrase(object: &CandidateTerm) -> String {
    if object.tags.iter().any(|tag| {
        matches!(
            tag.as_str(),
            "identity" | "belonging" | "romance" | "supernatural"
        )
    }) {
        format!("their {}", sentence_case(&object.value))
    } else if object.domain == NOUN_GROUP {
        format!("the {}", sentence_case(&object.value))
    } else {
        sentence_case(&object.value)
    }
}

fn format_stake_phrase(object: &CandidateTerm) -> String {
    if object.domain == NOUN_GROUP {
        format!("the {}", sentence_case(&object.value))
    } else {
        sentence_case(&object.value)
    }
}

fn record_matches_filters(record: &MotiveRecord, include: &[String], exclude: &[String]) -> bool {
    include
        .iter()
        .all(|tag| record.tags.iter().any(|own| own == tag))
        && exclude
            .iter()
            .all(|tag| record.tags.iter().all(|own| own != tag))
}

fn shares_any_tags(left: &[String], right: &[String]) -> bool {
    left.iter()
        .any(|tag| right.iter().any(|other| tag == other))
}

fn merge_dynamic_tags(tag_sets: &[&[String]], extra: &[&str]) -> Vec<String> {
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

fn merge_tag_slice(tags: &mut Vec<String>, extra: &[&str]) {
    let mut merged: BTreeSet<String> = tags.iter().cloned().collect();
    for tag in extra {
        merged.insert((*tag).to_string());
    }
    *tags = merged.into_iter().collect();
}

fn normalize_lookup(value: &str) -> String {
    value.trim().to_lowercase().replace(' ', "_")
}

fn normalize_lemma(value: &str) -> String {
    let normalized = value.trim().to_lowercase().replace('_', " ");
    if normalized.is_empty() || normalized.contains('(') || normalized.contains(')') {
        return String::new();
    }
    normalized
}

fn looks_noisy(value: &str) -> bool {
    let blocked = [
        "affright",
        "alarm",
        "assignment",
        "collectable",
        "collectible",
        "compunction",
        "condition",
        "confusion",
        "condominium",
        "custom",
        "curio",
        "domicile",
        "emotion",
        "exactitude",
        "existence",
        "fact",
        "frisson",
        "gift",
        "hungriness",
        "interest",
        "knickknack",
        "operator",
        "personality",
        "property",
        "quality",
        "trait",
    ];
    blocked.iter().any(|word| value == *word) || value.split_whitespace().count() > 2
}

fn is_usable_drive(value: &str) -> bool {
    is_simple_phrase(value, 2)
}

fn is_usable_action(value: &str) -> bool {
    is_simple_phrase(value, 2) && !value.contains('-')
}

fn is_usable_object(value: &str) -> bool {
    is_simple_phrase(value, 2)
}

fn is_usable_tension(value: &str) -> bool {
    is_simple_phrase(value, 2)
}

fn is_simple_phrase(value: &str, max_words: usize) -> bool {
    !value.is_empty()
        && value.split_whitespace().count() <= max_words
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphabetic() || ch == ' ' || ch == '-')
}

impl MotiveRecord {
    fn into_json(self) -> Value {
        let mut object = Map::new();
        object.insert("value".to_string(), Value::String(self.value));
        object.insert("kind".to_string(), Value::String(self.kind));
        object.insert(
            "tags".to_string(),
            Value::Array(self.tags.into_iter().map(Value::String).collect()),
        );
        if let Some(drive) = self.drive {
            object.insert("drive".to_string(), Value::String(drive));
        }
        if let Some(goal) = self.goal {
            object.insert("goal".to_string(), Value::String(goal));
        }
        if let Some(obstacle) = self.obstacle {
            object.insert("obstacle".to_string(), Value::String(obstacle));
        }
        if let Some(outcome) = self.outcome {
            object.insert("outcome".to_string(), Value::String(outcome));
        }
        if !self.forces.is_empty() {
            object.insert(
                "forces".to_string(),
                Value::Array(self.forces.into_iter().map(Value::String).collect()),
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

fn sentence_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{CharacterArchetypeArgs, WordArgs};
    use crate::{cmd_character_archetype, cmd_word};
    use serde_json::json;
    use std::collections::BTreeMap;
    use zacor_package::FromArgs;

    fn make_args<T: FromArgs>(pairs: &[(&str, Value)]) -> T {
        let map: BTreeMap<String, Value> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        T::from_args(&map).unwrap()
    }

    #[test]
    fn generates_one_record_by_default() {
        let args: CharacterMotiveArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_character_motive(&args).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn count_generates_multiple_records() {
        let args: CharacterMotiveArgs = make_args(&[("count", json!(3)), ("seed", json!(42))]);
        let result = cmd_character_motive(&args).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn seed_is_deterministic() {
        let args: CharacterMotiveArgs = make_args(&[("count", json!(4)), ("seed", json!(7))]);
        let a = cmd_character_motive(&args).unwrap();
        let b = cmd_character_motive(&args).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn narrative_and_forces_kinds_are_available() {
        let mut rng = crate::make_rng(Some(7.0));
        let mut saw_narrative = false;
        let mut saw_forces = false;
        for _ in 0..30 {
            let record = generate_record(&mut rng).unwrap();
            saw_narrative |= record.kind == "narrative";
            saw_forces |= record.kind == "forces";
        }
        assert!(saw_narrative);
        assert!(saw_forces);
    }

    #[test]
    fn collected_candidates_prefer_non_seed_distances() {
        let candidates =
            collect_candidates(&DRIVE_SEEDS[0], POS::Noun, DRIVE_DOMAINS, is_usable_drive);
        assert!(candidates.iter().any(|candidate| candidate.distance >= 1));
    }

    #[test]
    fn generated_records_do_not_use_old_static_phrases() {
        let args: CharacterMotiveArgs = make_args(&[("count", json!(12)), ("seed", json!(42))]);
        let result = cmd_character_motive(&args).unwrap();
        for record in result {
            let value = record["value"].as_str().unwrap();
            assert!(!value.contains("a rival stands in their way"));
            assert!(!value.contains("the law is already waiting for them to slip"));
            assert!(!value.contains("finally earn a place to call home"));
        }
    }

    #[test]
    fn strict_include_filter_is_applied() {
        let args: CharacterMotiveArgs = make_args(&[
            ("include", json!("authority,ambition")),
            ("count", json!(5)),
            ("seed", json!(42)),
        ]);
        let result = cmd_character_motive(&args).unwrap();
        for record in result {
            let tags = record["tags"].as_array().unwrap();
            assert!(tags.iter().any(|tag| tag.as_str() == Some("authority")));
            assert!(tags.iter().any(|tag| tag.as_str() == Some("ambition")));
        }
    }

    #[test]
    fn strict_exclude_filter_is_applied() {
        let args: CharacterMotiveArgs = make_args(&[
            ("exclude", json!("supernatural")),
            ("count", json!(5)),
            ("seed", json!(42)),
        ]);
        let result = cmd_character_motive(&args).unwrap();
        for record in result {
            let tags = record["tags"].as_array().unwrap();
            assert!(tags.iter().all(|tag| tag.as_str() != Some("supernatural")));
        }
    }

    #[test]
    fn archetype_behavior_remains_unchanged() {
        let args: CharacterArchetypeArgs = make_args(&[("count", json!(4)), ("seed", json!(42))]);
        let before = cmd_character_archetype(&args).unwrap();
        let motive_args: CharacterMotiveArgs =
            make_args(&[("count", json!(4)), ("seed", json!(42))]);
        let _ = cmd_character_motive(&motive_args).unwrap();
        let after = cmd_character_archetype(&args).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn rand_word_behavior_remains_unchanged() {
        let word_args: WordArgs = make_args(&[("count", json!(3)), ("seed", json!(42))]);
        let before = cmd_word(&word_args).unwrap();
        let motive_args: CharacterMotiveArgs =
            make_args(&[("count", json!(5)), ("seed", json!(42))]);
        let _ = cmd_character_motive(&motive_args).unwrap();
        let after = cmd_word(&word_args).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn package_metadata_contains_nested_motive_command() {
        let package_yaml = include_str!("../../package.yaml");
        assert!(package_yaml.contains("  character:\n"));
        assert!(package_yaml.contains("      motive:\n"));
    }
}
