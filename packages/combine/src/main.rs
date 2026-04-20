zacor_package::commands!("combine", {
    "append"  => zr_combine::cmd_append [input],
    "prepend" => zr_combine::cmd_prepend [input],
    "merge"   => zr_combine::cmd_merge [input],
    "join"    => zr_combine::cmd_join [input],
    "zip"     => zr_combine::cmd_zip [input],
});
