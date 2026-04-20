//! Random sentence generation from POS templates.
//!
//! Public surface: [`generate`]. Everything else is implementation detail.
//!
//! A sentence is built by parsing a template into [`template::Token`]s, then
//! filling each slot with a lemma drawn at random from WordNet under the
//! slot's POS / domain constraints, then applying English inflection
//! ([`inflect::pluralize`], [`inflect::third_singular`]) where the slot
//! requested it. The default template pool is [`templates::SENTENCE_TEMPLATES`].

mod inflect;
mod template;
mod templates;

use crate::models::{POS, domain_name};
use crate::wordnet::WordNet;
use rand::Rng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

use template::{Form, Token};

/// Generate one sentence. Returns `(text, template_used)`.
///
/// If `user_template` is `None`, picks one of the curated templates at random.
/// If `raw` is true, slots are filled with bare lemmas (no plural / 3sg
/// inflection); the template's form requests are still parsed but ignored.
pub fn generate(
    wn: &WordNet,
    user_template: Option<&str>,
    raw: bool,
    rng: &mut StdRng,
) -> Result<(String, String), String> {
    let template_str: String = match user_template {
        Some(t) => t.to_string(),
        None => templates::SENTENCE_TEMPLATES
            .choose(rng)
            .copied()
            .ok_or_else(|| "no curated templates available".to_string())?
            .to_string(),
    };

    let tokens = template::parse(&template_str)?;

    let mut out = String::new();
    for tok in &tokens {
        match tok {
            Token::Lit(s) => out.push_str(s),
            Token::Slot { pos, domain, form } => {
                let lemma = pick_lemma(wn, *pos, *domain, rng)?;
                let word = if raw {
                    lemma
                } else {
                    apply_form(&lemma, *form)
                };
                out.push_str(&word);
            }
        }
    }

    Ok((finalize(&out), template_str))
}

fn apply_form(lemma: &str, form: Form) -> String {
    match form {
        Form::Lemma => lemma.to_string(),
        Form::Plural => inflect::pluralize(lemma),
        Form::ThirdSingular => inflect::third_singular(lemma),
    }
}

/// Pick one random single-word lemma from WordNet matching the constraints.
///
/// Uses reservoir sampling (k=1) so we never allocate the candidate pool.
/// Multi-word lemmas (containing space, underscore, or hyphen) are skipped:
/// the inflection engine only handles single words and would mangle them.
fn pick_lemma(
    wn: &WordNet,
    pos: POS,
    domain: Option<u8>,
    rng: &mut StdRng,
) -> Result<String, String> {
    let mut chosen: Option<String> = None;
    let mut count: u64 = 0;
    for synset in wn.all_synsets(pos) {
        if let Some(d) = domain {
            if synset.domain != d {
                continue;
            }
        }
        for ws in &synset.word_senses {
            if ws.lemma.contains(' ') || ws.lemma.contains('_') || ws.lemma.contains('-') {
                continue;
            }
            count += 1;
            if rng.gen_range(0..count) == 0 {
                chosen = Some(ws.lemma.to_lowercase());
            }
        }
    }
    chosen.ok_or_else(|| {
        let dn = domain.map(domain_name).unwrap_or("any");
        format!("no lemmas found for pos={pos} domain={dn}")
    })
}

/// Trim, capitalize first letter, ensure terminal punctuation.
fn finalize(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut chars = trimmed.chars();
    let first: String = chars.next().unwrap().to_uppercase().collect();
    let rest: String = chars.collect();
    let mut out = format!("{first}{rest}");
    if !out.ends_with('.') && !out.ends_with('!') && !out.ends_with('?') {
        out.push('.');
    }
    out
}
