zacor_package::commands!("word", {
    "lookup"   => zr_word::cmd_lookup,
    "related"  => zr_word::cmd_related,
    "domain"   => zr_word::cmd_domain,
    "random"   => zr_word::cmd_random,
    "pattern"  => zr_word::cmd_pattern,
    "sentence" => zr_word::cmd_sentence,
});
