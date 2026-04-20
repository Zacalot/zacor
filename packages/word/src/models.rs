use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SynsetId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum POS {
    Adj,
    Noun,
    Verb,
    Adv,
}

impl POS {
    pub fn from_ss_type(c: char) -> Option<Self> {
        match c {
            'a' | 's' => Some(POS::Adj),
            'n' => Some(POS::Noun),
            'v' => Some(POS::Verb),
            'r' => Some(POS::Adv),
            _ => None,
        }
    }

    pub fn from_sense_type(n: u8) -> Option<Self> {
        match n {
            1 => Some(POS::Noun),
            2 => Some(POS::Verb),
            3 | 5 => Some(POS::Adj),
            4 => Some(POS::Adv),
            _ => None,
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "adj" | "adjective" | "a" => Some(POS::Adj),
            "noun" | "n" => Some(POS::Noun),
            "verb" | "v" => Some(POS::Verb),
            "adv" | "adverb" | "r" => Some(POS::Adv),
            _ => None,
        }
    }
}

impl fmt::Display for POS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            POS::Adj => write!(f, "adj"),
            POS::Noun => write!(f, "noun"),
            POS::Verb => write!(f, "verb"),
            POS::Adv => write!(f, "adv"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationType {
    Hypernym,
    InstanceHypernym,
    Hyponym,
    InstanceHyponym,
    Antonym,
    PartOf,
    SubstanceOf,
    MemberOf,
    HasPart,
    HasSubstance,
    HasMember,
    DerivationallyRelated,
    SimilarTo,
    AlsoSee,
    VerbGroup,
    Participle,
    Pertainym,
    DomainTopic,
    DomainRegion,
    DomainUsage,
    MemberOfDomainTopic,
    MemberOfDomainRegion,
    MemberOfDomainUsage,
    Attribute,
    Entailment,
    Cause,
}

impl RelationType {
    pub fn from_pointer_symbol(s: &str) -> Option<Self> {
        match s {
            "!" => Some(Self::Antonym),
            "@" => Some(Self::Hypernym),
            "@i" => Some(Self::InstanceHypernym),
            "~" => Some(Self::Hyponym),
            "~i" => Some(Self::InstanceHyponym),
            "#m" => Some(Self::HasMember),
            "#s" => Some(Self::HasSubstance),
            "#p" => Some(Self::HasPart),
            "%m" => Some(Self::MemberOf),
            "%s" => Some(Self::SubstanceOf),
            "%p" => Some(Self::PartOf),
            "=" => Some(Self::Attribute),
            "+" => Some(Self::DerivationallyRelated),
            ";c" => Some(Self::DomainTopic),
            "-c" => Some(Self::MemberOfDomainTopic),
            ";r" => Some(Self::DomainRegion),
            "-r" => Some(Self::MemberOfDomainRegion),
            ";u" => Some(Self::DomainUsage),
            "-u" => Some(Self::MemberOfDomainUsage),
            "&" => Some(Self::SimilarTo),
            "<" => Some(Self::Participle),
            "\\" => Some(Self::Pertainym),
            "^" => Some(Self::AlsoSee),
            "$" => Some(Self::VerbGroup),
            "*" => Some(Self::Entailment),
            ">" => Some(Self::Cause),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Hypernym | Self::InstanceHypernym => "hypernym",
            Self::Hyponym | Self::InstanceHyponym => "hyponym",
            Self::Antonym => "antonym",
            Self::PartOf | Self::SubstanceOf | Self::MemberOf => "part-of",
            Self::HasPart | Self::HasSubstance | Self::HasMember => "has-part",
            Self::DerivationallyRelated => "derivation",
            Self::SimilarTo => "similar",
            Self::AlsoSee => "also-see",
            Self::VerbGroup => "verb-group",
            Self::Participle => "participle",
            Self::Pertainym => "pertainym",
            Self::DomainTopic | Self::DomainRegion | Self::DomainUsage => "domain",
            Self::MemberOfDomainTopic | Self::MemberOfDomainRegion | Self::MemberOfDomainUsage => {
                "domain-member"
            }
            Self::Attribute => "attribute",
            Self::Entailment => "entailment",
            Self::Cause => "cause",
        }
    }

    pub fn from_filter(s: &str) -> Vec<Self> {
        match s.to_lowercase().as_str() {
            "synonym" => vec![], // handled specially
            "antonym" => vec![Self::Antonym],
            "hypernym" => vec![Self::Hypernym, Self::InstanceHypernym],
            "hyponym" => vec![Self::Hyponym, Self::InstanceHyponym],
            "derivation" => vec![Self::DerivationallyRelated],
            "similar" => vec![Self::SimilarTo],
            "also-see" => vec![Self::AlsoSee],
            "domain" => vec![Self::DomainTopic, Self::DomainRegion, Self::DomainUsage],
            "part-of" => vec![Self::PartOf, Self::SubstanceOf, Self::MemberOf],
            "has-part" => vec![Self::HasPart, Self::HasSubstance, Self::HasMember],
            "member-of" => vec![Self::MemberOf],
            "has-member" => vec![Self::HasMember],
            _ => vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct WordSense {
    pub lemma: String,
    pub frequency: u32,
}

#[derive(Debug, Clone)]
pub struct Synset {
    pub id: SynsetId,
    pub pos: POS,
    pub domain: u8,
    pub word_senses: Vec<WordSense>,
    pub definition: String,
    pub examples: Vec<String>,
    pub relations: Vec<(RelationType, POS, SynsetId)>,
}

pub struct PosData {
    pub synsets: Vec<Synset>,
    pub offset_index: Vec<(u32, usize)>, // sorted by offset for binary search
}

impl PosData {
    pub fn new(synsets: Vec<Synset>) -> Self {
        let mut offset_index: Vec<(u32, usize)> = synsets
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.0, i))
            .collect();
        offset_index.sort_unstable_by_key(|&(offset, _)| offset);
        PosData {
            synsets,
            offset_index,
        }
    }

    pub fn get(&self, id: SynsetId) -> Option<&Synset> {
        self.offset_index
            .binary_search_by_key(&id.0, |&(offset, _)| offset)
            .ok()
            .map(|i| &self.synsets[self.offset_index[i].1])
    }
}

/// WordNet lexicographer file number → domain name
pub fn domain_name(num: u8) -> &'static str {
    match num {
        0 => "adj.all",
        1 => "adj.pert",
        2 => "adv.all",
        3 => "noun.Tops",
        4 => "noun.act",
        5 => "noun.animal",
        6 => "noun.artifact",
        7 => "noun.attribute",
        8 => "noun.body",
        9 => "noun.cognition",
        10 => "noun.communication",
        11 => "noun.event",
        12 => "noun.feeling",
        13 => "noun.food",
        14 => "noun.group",
        15 => "noun.location",
        16 => "noun.motive",
        17 => "noun.object",
        18 => "noun.person",
        19 => "noun.phenomenon",
        20 => "noun.plant",
        21 => "noun.possession",
        22 => "noun.process",
        23 => "noun.quantity",
        24 => "noun.relation",
        25 => "noun.shape",
        26 => "noun.state",
        27 => "noun.substance",
        28 => "noun.time",
        29 => "verb.body",
        30 => "verb.change",
        31 => "verb.cognition",
        32 => "verb.communication",
        33 => "verb.competition",
        34 => "verb.consumption",
        35 => "verb.contact",
        36 => "verb.creation",
        37 => "verb.emotion",
        38 => "verb.motion",
        39 => "verb.perception",
        40 => "verb.possession",
        41 => "verb.social",
        42 => "verb.stative",
        43 => "verb.weather",
        44 => "adj.ppl",
        _ => "unknown",
    }
}

pub const DOMAIN_COUNT: usize = 45;

/// All 45 domain names for iteration
pub fn all_domain_names() -> &'static [&'static str; 45] {
    &[
        "adj.all",
        "adj.pert",
        "adv.all",
        "noun.Tops",
        "noun.act",
        "noun.animal",
        "noun.artifact",
        "noun.attribute",
        "noun.body",
        "noun.cognition",
        "noun.communication",
        "noun.event",
        "noun.feeling",
        "noun.food",
        "noun.group",
        "noun.location",
        "noun.motive",
        "noun.object",
        "noun.person",
        "noun.phenomenon",
        "noun.plant",
        "noun.possession",
        "noun.process",
        "noun.quantity",
        "noun.relation",
        "noun.shape",
        "noun.state",
        "noun.substance",
        "noun.time",
        "verb.body",
        "verb.change",
        "verb.cognition",
        "verb.communication",
        "verb.competition",
        "verb.consumption",
        "verb.contact",
        "verb.creation",
        "verb.emotion",
        "verb.motion",
        "verb.perception",
        "verb.possession",
        "verb.social",
        "verb.stative",
        "verb.weather",
        "adj.ppl",
    ]
}

/// Match a domain name flexibly (case-insensitive, dot/underscore)
pub fn match_domain(input: &str) -> Option<u8> {
    let normalized = input.to_lowercase().replace('_', ".");
    for (i, name) in all_domain_names().iter().enumerate() {
        if name.to_lowercase() == normalized {
            return Some(i as u8);
        }
    }
    None
}
