//! Tiny English inflection engine. Rules-based, ASCII-only.
//!
//! Two operations are exposed: pluralizing nouns and forming the
//! third-person singular present of verbs (the `-s` form). Each is a
//! handful of suffix rules plus a list of irregulars. Both consume and
//! produce lowercase strings; multi-word lemmas should be filtered out
//! before reaching here.

const PLURAL_IRREGULARS: &[(&str, &str)] = &[
    ("child", "children"),
    ("man", "men"),
    ("woman", "women"),
    ("foot", "feet"),
    ("tooth", "teeth"),
    ("mouse", "mice"),
    ("goose", "geese"),
    ("person", "people"),
    ("ox", "oxen"),
    ("die", "dice"),
    ("cactus", "cacti"),
    ("fungus", "fungi"),
    ("nucleus", "nuclei"),
    ("syllabus", "syllabi"),
    ("analysis", "analyses"),
    ("crisis", "crises"),
    ("thesis", "theses"),
    ("phenomenon", "phenomena"),
    ("criterion", "criteria"),
    ("datum", "data"),
    ("leaf", "leaves"),
    ("life", "lives"),
    ("knife", "knives"),
    ("wife", "wives"),
    ("wolf", "wolves"),
    ("sheep", "sheep"),
    ("deer", "deer"),
    ("fish", "fish"),
];

const VERB_3SG_IRREGULARS: &[(&str, &str)] = &[
    ("be", "is"),
    ("have", "has"),
];

pub fn pluralize(lemma: &str) -> String {
    let lower = lemma.to_lowercase();
    if let Some(v) = lookup(&lower, PLURAL_IRREGULARS) {
        return v.to_string();
    }
    if !lower.is_ascii() {
        return format!("{lower}s");
    }
    let bytes = lower.as_bytes();
    let n = bytes.len();
    if n == 0 {
        return lower;
    }
    if n >= 2 && bytes[n - 1] == b'y' && !is_vowel(bytes[n - 2] as char) {
        return format!("{}ies", &lower[..n - 1]);
    }
    if lower.ends_with("ch")
        || lower.ends_with("sh")
        || lower.ends_with('s')
        || lower.ends_with('x')
        || lower.ends_with('z')
    {
        return format!("{lower}es");
    }
    format!("{lower}s")
}

pub fn third_singular(lemma: &str) -> String {
    let lower = lemma.to_lowercase();
    if let Some(v) = lookup(&lower, VERB_3SG_IRREGULARS) {
        return v.to_string();
    }
    if !lower.is_ascii() {
        return format!("{lower}s");
    }
    let bytes = lower.as_bytes();
    let n = bytes.len();
    if n == 0 {
        return lower;
    }
    if n >= 2 && bytes[n - 1] == b'y' && !is_vowel(bytes[n - 2] as char) {
        return format!("{}ies", &lower[..n - 1]);
    }
    if lower.ends_with("ch")
        || lower.ends_with("sh")
        || lower.ends_with('s')
        || lower.ends_with('x')
        || lower.ends_with('z')
        || lower.ends_with('o')
    {
        return format!("{lower}es");
    }
    format!("{lower}s")
}

fn lookup<'a>(word: &str, table: &'a [(&'a str, &'a str)]) -> Option<&'a str> {
    table.iter().find(|(k, _)| *k == word).map(|(_, v)| *v)
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plural_regular() {
        assert_eq!(pluralize("cat"), "cats");
        assert_eq!(pluralize("dog"), "dogs");
        assert_eq!(pluralize("otter"), "otters");
    }

    #[test]
    fn plural_sibilants() {
        assert_eq!(pluralize("box"), "boxes");
        assert_eq!(pluralize("bus"), "buses");
        assert_eq!(pluralize("buzz"), "buzzes");
        assert_eq!(pluralize("church"), "churches");
        assert_eq!(pluralize("dish"), "dishes");
    }

    #[test]
    fn plural_y_after_consonant() {
        assert_eq!(pluralize("city"), "cities");
        assert_eq!(pluralize("party"), "parties");
    }

    #[test]
    fn plural_y_after_vowel() {
        assert_eq!(pluralize("day"), "days");
        assert_eq!(pluralize("toy"), "toys");
    }

    #[test]
    fn plural_irregulars() {
        assert_eq!(pluralize("child"), "children");
        assert_eq!(pluralize("man"), "men");
        assert_eq!(pluralize("mouse"), "mice");
        assert_eq!(pluralize("leaf"), "leaves");
        assert_eq!(pluralize("sheep"), "sheep");
    }

    #[test]
    fn third_sg_regular() {
        assert_eq!(third_singular("run"), "runs");
        assert_eq!(third_singular("walk"), "walks");
        assert_eq!(third_singular("saunter"), "saunters");
    }

    #[test]
    fn third_sg_sibilants_and_o() {
        assert_eq!(third_singular("buzz"), "buzzes");
        assert_eq!(third_singular("watch"), "watches");
        assert_eq!(third_singular("wash"), "washes");
        assert_eq!(third_singular("go"), "goes");
        assert_eq!(third_singular("do"), "does");
    }

    #[test]
    fn third_sg_y_after_consonant() {
        assert_eq!(third_singular("try"), "tries");
        assert_eq!(third_singular("fly"), "flies");
    }

    #[test]
    fn third_sg_irregulars() {
        assert_eq!(third_singular("be"), "is");
        assert_eq!(third_singular("have"), "has");
    }
}
