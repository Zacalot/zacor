use super::data::SemanticSeed;

pub struct NarrativeTemplate {
    pub base: &'static str,
    pub with_obstacle: &'static str,
    pub with_outcome: &'static str,
    pub with_both: &'static str,
}

pub const DRIVE_SEEDS: &[SemanticSeed] = &[
    SemanticSeed {
        seed: "ambition",
        tags: &["ambition", "authority", "power"],
    },
    SemanticSeed {
        seed: "love",
        tags: &["romance", "belonging"],
    },
    SemanticSeed {
        seed: "vengeance",
        tags: &["vengeance", "threat", "justice"],
    },
    SemanticSeed {
        seed: "duty",
        tags: &["authority", "duty", "protector"],
    },
    SemanticSeed {
        seed: "survival",
        tags: &["survival", "outcast"],
    },
    SemanticSeed {
        seed: "curiosity",
        tags: &["knowledge", "supernatural"],
    },
    SemanticSeed {
        seed: "greed",
        tags: &["criminal", "wealth"],
    },
    SemanticSeed {
        seed: "redemption",
        tags: &["redemption", "outcast", "justice"],
    },
    SemanticSeed {
        seed: "lust",
        tags: &["romance", "threat"],
    },
    SemanticSeed {
        seed: "obedience",
        tags: &["authority", "duty"],
    },
    SemanticSeed {
        seed: "envy",
        tags: &["ambition", "threat"],
    },
    SemanticSeed {
        seed: "regret",
        tags: &["redemption", "outcast"],
    },
    SemanticSeed {
        seed: "faith",
        tags: &["supernatural", "duty"],
    },
    SemanticSeed {
        seed: "fear",
        tags: &["survival", "threat"],
    },
    SemanticSeed {
        seed: "desire",
        tags: &["romance", "ambition"],
    },
];

pub const GOAL_ACTION_SEEDS: &[SemanticSeed] = &[
    SemanticSeed {
        seed: "claim",
        tags: &["authority", "ambition", "power"],
    },
    SemanticSeed {
        seed: "protect",
        tags: &["protector", "duty", "authority"],
    },
    SemanticSeed {
        seed: "restore",
        tags: &["redemption", "justice"],
    },
    SemanticSeed {
        seed: "uncover",
        tags: &["knowledge", "justice"],
    },
    SemanticSeed {
        seed: "escape",
        tags: &["freedom", "outcast"],
    },
    SemanticSeed {
        seed: "seize",
        tags: &["criminal", "wealth", "power"],
    },
    SemanticSeed {
        seed: "save",
        tags: &["protector", "romance"],
    },
    SemanticSeed {
        seed: "discover",
        tags: &["knowledge", "supernatural"],
    },
    SemanticSeed {
        seed: "avenge",
        tags: &["vengeance", "justice", "threat"],
    },
    SemanticSeed {
        seed: "conquer",
        tags: &["authority", "power", "threat"],
    },
    SemanticSeed {
        seed: "redeem",
        tags: &["redemption", "justice"],
    },
    SemanticSeed {
        seed: "secure",
        tags: &["authority", "wealth", "belonging"],
    },
];

pub const GOAL_OBJECT_SEEDS: &[SemanticSeed] = &[
    SemanticSeed {
        seed: "power",
        tags: &["authority", "ambition", "power"],
    },
    SemanticSeed {
        seed: "belonging",
        tags: &["belonging", "freedom", "outcast"],
    },
    SemanticSeed {
        seed: "truth",
        tags: &["knowledge", "justice"],
    },
    SemanticSeed {
        seed: "fortune",
        tags: &["wealth", "criminal", "freedom"],
    },
    SemanticSeed {
        seed: "identity",
        tags: &["authority", "justice", "redemption"],
    },
    SemanticSeed {
        seed: "safety",
        tags: &["authority", "protector", "duty"],
    },
    SemanticSeed {
        seed: "freedom",
        tags: &["freedom", "outcast"],
    },
    SemanticSeed {
        seed: "inheritance",
        tags: &["supernatural", "redemption"],
    },
    SemanticSeed {
        seed: "authority",
        tags: &["authority", "power"],
    },
    SemanticSeed {
        seed: "devotion",
        tags: &["romance", "belonging"],
    },
    SemanticSeed {
        seed: "justice",
        tags: &["vengeance", "threat"],
    },
    SemanticSeed {
        seed: "legacy",
        tags: &["redemption", "justice", "identity"],
    },
];

pub const FORCE_TENSION_SEEDS: &[SemanticSeed] = &[
    SemanticSeed {
        seed: "poverty",
        tags: &["survival", "wealth", "outcast"],
    },
    SemanticSeed {
        seed: "madness",
        tags: &["supernatural", "threat"],
    },
    SemanticSeed {
        seed: "doubt",
        tags: &["knowledge", "supernatural"],
    },
    SemanticSeed {
        seed: "shame",
        tags: &["redemption", "outcast"],
    },
    SemanticSeed {
        seed: "mercy",
        tags: &["justice", "romance"],
    },
    SemanticSeed {
        seed: "rebellion",
        tags: &["authority", "freedom", "threat"],
    },
    SemanticSeed {
        seed: "hope",
        tags: &["redemption", "belonging"],
    },
    SemanticSeed {
        seed: "hunger",
        tags: &["survival", "threat"],
    },
    SemanticSeed {
        seed: "grace",
        tags: &["romance", "justice"],
    },
    SemanticSeed {
        seed: "chaos",
        tags: &["threat", "outcast"],
    },
];

pub const NARRATIVE_TEMPLATES: &[NarrativeTemplate] = &[
    NarrativeTemplate {
        base: "The character is driven by {drive}. Their goal is to {goal}.",
        with_obstacle: "The character is driven by {drive}. Their goal is to {goal}. But {obstacle}.",
        with_outcome: "The character is driven by {drive}. Their goal is to {goal}. If they succeed, {outcome}.",
        with_both: "The character is driven by {drive}. Their goal is to {goal}. But {obstacle}. If they succeed, {outcome}.",
    },
    NarrativeTemplate {
        base: "Driven by {drive}, they want to {goal}.",
        with_obstacle: "Driven by {drive}, they want to {goal}. Yet {obstacle}.",
        with_outcome: "Driven by {drive}, they want to {goal}. Success could mean {outcome}.",
        with_both: "Driven by {drive}, they want to {goal}. Yet {obstacle}. Success could mean {outcome}.",
    },
    NarrativeTemplate {
        base: "{drive} pushes them to {goal}.",
        with_obstacle: "{drive} pushes them to {goal}, but {obstacle}.",
        with_outcome: "{drive} pushes them to {goal}, and victory could let them {outcome}.",
        with_both: "{drive} pushes them to {goal}, but {obstacle}. Even then, {outcome}.",
    },
    NarrativeTemplate {
        base: "Because of {drive}, they are trying to {goal}.",
        with_obstacle: "Because of {drive}, they are trying to {goal}. However, {obstacle}.",
        with_outcome: "Because of {drive}, they are trying to {goal}. If they pull it off, {outcome}.",
        with_both: "Because of {drive}, they are trying to {goal}. However, {obstacle}. If they pull it off, {outcome}.",
    },
];

pub const OBSTACLE_TEMPLATES: &[&str] = &[
    "{tension} makes them {action} {object}",
    "{tension} pushes them to {action} {object}",
    "{tension} keeps urging them to {action} {object}",
    "{tension} threatens {object} and drives them to {action} {stake}",
];

pub const OUTCOME_TEMPLATES: &[&str] = &[
    "{action} {object}",
    "finally {action} {object}",
    "still {action} {object}",
    "be forced to {action} {object}",
];
