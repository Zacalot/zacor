zacor_package::commands!("reshape", {
    "rename"    => zr_reshape::cmd_rename [input],
    "flatten"   => zr_reshape::cmd_flatten [input],
    "transpose" => zr_reshape::cmd_transpose [input],
    "wrap"      => zr_reshape::cmd_wrap [input],
    "group-by"  => zr_reshape::cmd_group_by [input],
    "enumerate" => zr_reshape::cmd_enumerate [input],
    "columns"   => zr_reshape::cmd_columns [input],
    "values"    => zr_reshape::cmd_values [input],
});
