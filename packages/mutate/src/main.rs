zacor_package::commands!("mutate", {
    "insert" => zr_mutate::cmd_insert [input],
    "update" => zr_mutate::cmd_update [input],
    "upsert" => zr_mutate::cmd_upsert [input],
});
