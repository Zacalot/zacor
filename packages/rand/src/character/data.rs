pub struct CuratedLabel {
    pub label: &'static str,
    pub tags: &'static [&'static str],
}

pub struct SemanticSeed {
    pub seed: &'static str,
    pub tags: &'static [&'static str],
}

pub const ROLE_SEEDS: &[SemanticSeed] = &[
    SemanticSeed {
        seed: "leader",
        tags: &["role", "authority"],
    },
    SemanticSeed {
        seed: "detective",
        tags: &["role", "investigator"],
    },
    SemanticSeed {
        seed: "singer",
        tags: &["role", "creative", "romance"],
    },
    SemanticSeed {
        seed: "soldier",
        tags: &["role", "authority", "threat"],
    },
    SemanticSeed {
        seed: "guardian",
        tags: &["role", "authority", "protector"],
    },
    SemanticSeed {
        seed: "healer",
        tags: &["role", "caretaker"],
    },
    SemanticSeed {
        seed: "scholar",
        tags: &["role", "knowledge"],
    },
    SemanticSeed {
        seed: "merchant",
        tags: &["role", "occupation"],
    },
    SemanticSeed {
        seed: "wanderer",
        tags: &["role", "outcast"],
    },
    SemanticSeed {
        seed: "outlaw",
        tags: &["role", "criminal", "outcast", "threat"],
    },
    SemanticSeed {
        seed: "mystic",
        tags: &["role", "supernatural"],
    },
    SemanticSeed {
        seed: "monster",
        tags: &["role", "supernatural", "threat"],
    },
];

pub const TRAIT_SEEDS: &[SemanticSeed] = &[
    SemanticSeed {
        seed: "loyal",
        tags: &["trait"],
    },
    SemanticSeed {
        seed: "adaptable",
        tags: &["trait"],
    },
    SemanticSeed {
        seed: "charming",
        tags: &["trait", "romance"],
    },
    SemanticSeed {
        seed: "reckless",
        tags: &["trait", "threat"],
    },
    SemanticSeed {
        seed: "cunning",
        tags: &["trait", "criminal"],
    },
    SemanticSeed {
        seed: "brooding",
        tags: &["trait", "outcast"],
    },
    SemanticSeed {
        seed: "haunted",
        tags: &["trait", "supernatural"],
    },
    SemanticSeed {
        seed: "idealistic",
        tags: &["trait"],
    },
    SemanticSeed {
        seed: "obsessed",
        tags: &["trait", "threat"],
    },
    SemanticSeed {
        seed: "noble",
        tags: &["trait", "authority"],
    },
    SemanticSeed {
        seed: "wild",
        tags: &["trait", "outcast"],
    },
    SemanticSeed {
        seed: "romantic",
        tags: &["trait", "romance"],
    },
    SemanticSeed {
        seed: "secretive",
        tags: &["trait", "criminal", "outcast"],
    },
    SemanticSeed {
        seed: "naive",
        tags: &["trait", "youth"],
    },
];

pub const CURATED_LABELS: &[CuratedLabel] = &[
    CuratedLabel {
        label: "Chosen One",
        tags: &["trope", "identity", "supernatural"],
    },
    CuratedLabel {
        label: "Lone Wolf",
        tags: &["trope", "outcast", "threat"],
    },
    CuratedLabel {
        label: "Modern Romantic",
        tags: &["trope", "romance", "identity"],
    },
    CuratedLabel {
        label: "Crime Lord",
        tags: &["trope", "criminal", "authority", "threat"],
    },
    CuratedLabel {
        label: "Ghost Bride",
        tags: &["trope", "romance", "supernatural"],
    },
    CuratedLabel {
        label: "Runaway Heir",
        tags: &["trope", "identity", "outcast", "youth"],
    },
    CuratedLabel {
        label: "Witch Hunter",
        tags: &["trope", "authority", "supernatural", "threat"],
    },
    CuratedLabel {
        label: "Street Prophet",
        tags: &["trope", "outcast", "supernatural"],
    },
    CuratedLabel {
        label: "Reluctant Monarch",
        tags: &["trope", "authority", "identity"],
    },
    CuratedLabel {
        label: "Monster Scholar",
        tags: &["trope", "supernatural", "knowledge", "threat"],
    },
    CuratedLabel {
        label: "Fallen Heiress",
        tags: &["trope", "identity", "outcast", "romance"],
    },
    CuratedLabel {
        label: "Doomed Medium",
        tags: &["trope", "supernatural", "threat"],
    },
    CuratedLabel {
        label: "Smuggler Prince",
        tags: &["trope", "criminal", "romance", "authority"],
    },
];
