zacor_package::commands!("filter", {
    "skip"    => zr_filter::cmd_skip [input],
    "drop"    => zr_filter::cmd_drop [input],
    "uniq"    => zr_filter::cmd_uniq [input],
    "uniq-by" => zr_filter::cmd_uniq_by [input],
    "compact" => zr_filter::cmd_compact [input],
    "find"    => zr_filter::cmd_find [input],
});
