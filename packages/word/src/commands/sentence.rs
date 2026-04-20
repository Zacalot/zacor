use crate::args::SentenceArgs;
use crate::sentence;
use crate::wordnet::WordNet;
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde_json::Value;
use zacor_package::json;

pub fn cmd_sentence(args: &SentenceArgs) -> Result<Vec<Value>, String> {
    let wn = WordNet::embedded();
    let count = args.count.unwrap_or(1).max(1) as usize;
    let raw = args.raw;
    let user_template = args.template.as_deref();

    let mut rng: StdRng = match args.seed {
        Some(s) => StdRng::seed_from_u64(s as u64),
        None => StdRng::from_entropy(),
    };

    let mut results = Vec::with_capacity(count);
    for _ in 0..count {
        let (text, template) = sentence::generate(wn, user_template, raw, &mut rng)?;
        results.push(json!({
            "value": text,
            "template": template,
        }));
    }
    Ok(results)
}

#[cfg(all(test, feature = "embedded-data"))]
mod tests {
    use super::*;

    fn make_args(
        template: Option<&str>,
        raw: bool,
        count: Option<i64>,
        seed: Option<f64>,
    ) -> SentenceArgs {
        SentenceArgs {
            template: template.map(|s| s.to_string()),
            raw,
            count,
            seed,
        }
    }

    #[test]
    fn curated_default() {
        let r = cmd_sentence(&make_args(None, false, Some(1), Some(42.0))).unwrap();
        assert_eq!(r.len(), 1);
        let s = r[0]["value"].as_str().unwrap();
        assert!(!s.is_empty());
        assert!(s.chars().next().unwrap().is_uppercase());
        assert!(s.ends_with('.') || s.ends_with('!') || s.ends_with('?'));
    }

    #[test]
    fn user_template() {
        let r = cmd_sentence(&make_args(
            Some("the {noun:animal} {verb:motion:3sg}"),
            false,
            Some(1),
            Some(42.0),
        ))
        .unwrap();
        let s = r[0]["value"].as_str().unwrap();
        assert!(s.starts_with("The "));
        assert!(s.ends_with('.'));
        // 3sg should produce an -s ending on the verb (with a few exceptions
        // we don't try to assert here).
    }

    #[test]
    fn count() {
        let r = cmd_sentence(&make_args(None, false, Some(5), Some(42.0))).unwrap();
        assert_eq!(r.len(), 5);
    }

    #[test]
    fn seed_determinism() {
        let a = cmd_sentence(&make_args(None, false, Some(3), Some(7.0))).unwrap();
        let b = cmd_sentence(&make_args(None, false, Some(3), Some(7.0))).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn template_field_present() {
        let r = cmd_sentence(&make_args(
            Some("the {noun}"),
            false,
            Some(1),
            Some(1.0),
        ))
        .unwrap();
        assert_eq!(r[0]["template"], "the {noun}");
    }

    #[test]
    fn raw_skips_inflection() {
        // With --raw, {verb:3sg} should fall back to the lemma. We can't easily
        // assert that without knowing the lemma, but we can at least verify
        // generation succeeds and produces a non-empty string.
        let r = cmd_sentence(&make_args(
            Some("the {noun} {verb:3sg}"),
            true,
            Some(1),
            Some(99.0),
        ))
        .unwrap();
        assert!(!r[0]["value"].as_str().unwrap().is_empty());
    }
}
