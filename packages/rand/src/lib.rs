zacor_package::include_args!();

mod character;
pub(crate) mod data;
mod generators;
mod locale;
mod transformers;

pub use character::cmd_character_archetype;
pub use character::cmd_character_motive;
pub use generators::{
    cmd_bool, cmd_char, cmd_color, cmd_date, cmd_float, cmd_int, cmd_name, cmd_pass, cmd_pattern,
    cmd_phrase, cmd_syllable, cmd_uuid, cmd_word,
};
pub use transformers::{cmd_pick, cmd_shuffle};

use rand::SeedableRng;
use rand::rngs::StdRng;

pub(crate) fn make_rng(seed: Option<f64>) -> StdRng {
    match seed {
        Some(s) => StdRng::seed_from_u64(s as u64),
        None => StdRng::from_entropy(),
    }
}
