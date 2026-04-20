//! Curated sentence templates. Hand-written, hardcoded — no template packs,
//! no YAML, no runtime loading. Adding a template means editing this list.
//!
//! Each template uses the slot syntax documented in `template.rs`. Articles
//! ("the", "a", "an") are written literally; templates lean on "the" to dodge
//! the a/an problem since slot fills are random. All slots use POS + WordNet
//! lexicographer domains so the output stays loosely on-topic for each slot.

pub static SENTENCE_TEMPLATES: &[&str] = &[
    "the {adj} {noun:animal} {verb:motion:3sg} into the {noun:location}",
    "the {adj} {noun:person} {verb:3sg} the {adj} {noun:artifact}",
    "the {noun:person} {verb:communication:3sg} about the {noun:cognition}",
    "the {adj} {noun:plant:pl} grow beside the {adj} {noun:location}",
    "the {noun:animal} {verb:consumption:3sg} the {noun:food}",
    "every {noun:person} {verb:emotion:3sg} the {adj} {noun:phenomenon}",
    "the old {noun:person} {verb:3sg} the {adj} {noun:artifact}",
    "beneath the {noun:location}, the {noun:animal} {verb:3sg}",
    "the {noun:person:pl} and the {noun:animal:pl} {verb:motion} together",
    "the {adj} {noun:body} of the {noun:animal} {verb:3sg}",
    "no one {verb:3sg} the {adj} {noun:cognition}",
    "the {noun:person:pl} {verb:competition} over the {noun:possession}",
    "her {noun:body} {verb:3sg} like the {noun:object}",
    "the {noun:phenomenon} {verb:weather:3sg} across the {noun:location}",
    "the ancient {noun:person} {verb:creation:3sg} the {adj} {noun:artifact}",
    "the {adj} {noun:food} {verb:3sg} on the {noun:artifact}",
    "the {noun:animal:pl} {verb:perception} the {adj} {noun:object}",
    "the {adj} {noun:object} {verb:contact:3sg} the {noun:body}",
    "the {noun:person} who {verb:cognition:3sg} the {noun:cognition}",
    "{adj} and {adj}, the {noun:person} {verb:3sg}",
    "the {noun:animal} {verb:3sg} the {adj} {noun:plant}",
    "every morning the {noun:person} {verb:3sg} the {noun:artifact}",
    "the {noun:event} {verb:3sg} the {adj} {noun:group}",
    "she {verb:emotion:3sg} the {adj} {noun:phenomenon}",
    "they {verb:motion} through the {adj} {noun:location}",
    "the {noun:substance} {verb:change:3sg} into the {noun:substance}",
    "the {adj} {noun:person} {verb:possession:3sg} the {noun:artifact}",
    "the {noun:plant} {verb:3sg} toward the {noun:phenomenon}",
    "the child {verb:3sg} the {adj} {noun:animal}",
    "the {noun:person} {verb:stative:3sg} in the {noun:state}",
];
