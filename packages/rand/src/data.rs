use std::sync::OnceLock;

// --- Pool enum ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pool {
    Standard,
    Extended,
}

impl Pool {
    pub fn from_str(s: &str) -> Result<Pool, String> {
        match s {
            "standard" => Ok(Pool::Standard),
            "extended" => Ok(Pool::Extended),
            other => Err(format!(
                "rand: unknown pool '{other}'. Valid: standard, extended"
            )),
        }
    }
}

// --- Standard data (en-US) ---

const WORDS_RAW: &str = include_str!("../data/en-US/words.txt");
const FIRSTNAMES_RAW: &str = include_str!("../data/en-US/firstnames.txt");
const LASTNAMES_RAW: &str = include_str!("../data/en-US/lastnames.txt");

// --- Extended data (en-US) ---

const WORDS_EXTENDED_RAW: &str = include_str!("../data/en-US/words-extended.txt");
const NAMES_EXTENDED_RAW: &str = include_str!("../data/en-US/names-extended.txt");

// --- Lazy statics ---

static WORDS: OnceLock<Vec<&'static str>> = OnceLock::new();
static FIRSTNAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
static LASTNAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
static WORDS_EXTENDED: OnceLock<Vec<&'static str>> = OnceLock::new();
static NAMES_EXTENDED: OnceLock<Vec<&'static str>> = OnceLock::new();

fn load(raw: &'static str) -> Vec<&'static str> {
    raw.lines().filter(|l| !l.is_empty()).collect()
}

pub fn words(pool: Pool) -> &'static [&'static str] {
    match pool {
        Pool::Standard => WORDS.get_or_init(|| load(WORDS_RAW)),
        Pool::Extended => WORDS_EXTENDED.get_or_init(|| load(WORDS_EXTENDED_RAW)),
    }
}

pub fn firstnames(pool: Pool) -> &'static [&'static str] {
    match pool {
        Pool::Standard => FIRSTNAMES.get_or_init(|| load(FIRSTNAMES_RAW)),
        Pool::Extended => NAMES_EXTENDED.get_or_init(|| load(NAMES_EXTENDED_RAW)),
    }
}

pub fn lastnames(_pool: Pool) -> &'static [&'static str] {
    LASTNAMES.get_or_init(|| load(LASTNAMES_RAW))
}

// --- Syllable system ---

pub struct SyllableSet {
    pub onsets: &'static [&'static str],
    pub nuclei: &'static [&'static str],
    pub codas: &'static [&'static str],
    pub min_syllables: u8,
    pub max_syllables: u8,
}

pub static ENGLISH: SyllableSet = SyllableSet {
    onsets: &[
        "", "b", "bl", "br", "c", "ch", "cl", "cr", "d", "dr", "f", "fl", "fr", "g", "gl", "gr",
        "h", "j", "k", "kn", "l", "m", "n", "p", "pl", "pr", "qu", "r", "s", "sc", "sh", "sk",
        "sl", "sm", "sn", "sp", "st", "str", "sw", "t", "th", "tr", "tw", "v", "w", "wh", "wr",
        "y", "z",
    ],
    nuclei: &[
        "a", "e", "i", "o", "u", "ai", "ea", "ee", "oo", "ou", "oi", "ay", "ey", "ie", "ow",
    ],
    codas: &[
        "", "b", "ck", "d", "f", "g", "k", "l", "ll", "m", "n", "nd", "ng", "nk", "nt", "p", "r",
        "rd", "rn", "rt", "s", "sh", "sk", "sp", "st", "t", "th", "x", "z",
    ],
    min_syllables: 2,
    max_syllables: 3,
};

pub static FANTASY: SyllableSet = SyllableSet {
    onsets: &[
        "", "b", "ch", "d", "dr", "dh", "f", "g", "gh", "gl", "gr", "h", "j", "k", "kh", "kr", "l",
        "m", "n", "ph", "r", "rh", "s", "sh", "sk", "t", "th", "tr", "v", "vr", "w", "x", "z",
        "zh", "zr",
    ],
    nuclei: &[
        "a", "e", "i", "o", "u", "ae", "ai", "au", "ei", "ia", "ie", "io", "oa", "oo", "ua", "y",
    ],
    codas: &[
        "", "b", "d", "g", "k", "kk", "l", "ll", "m", "n", "nd", "ng", "nn", "r", "rn", "s", "sh",
        "ss", "th", "x", "z", "zz",
    ],
    min_syllables: 2,
    max_syllables: 3,
};

pub static SIMPLE: SyllableSet = SyllableSet {
    onsets: &[
        "b", "c", "d", "f", "g", "h", "j", "k", "l", "m", "n", "p", "r", "s", "t", "v", "w", "z",
    ],
    nuclei: &["a", "e", "i", "o", "u"],
    codas: &[
        "b", "d", "f", "g", "k", "l", "m", "n", "p", "r", "s", "t", "x", "z",
    ],
    min_syllables: 1,
    max_syllables: 1,
};

pub fn syllable_set(name: &str) -> Result<&'static SyllableSet, String> {
    match name {
        "english" => Ok(&ENGLISH),
        "fantasy" => Ok(&FANTASY),
        "simple" => Ok(&SIMPLE),
        other => Err(format!(
            "rand syllable: unknown set '{other}'. Valid: english, fantasy, simple, input"
        )),
    }
}
