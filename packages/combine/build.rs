fn main() {
    use zacor_package_build::*;

    let file_arg = ArgSchemaInfo::string("file").optional();
    let records_arg = ArgSchemaInfo::string("records").optional();

    PackageSpec::from_cargo("combine")
        .command(CommandSpec::named("append").description("Append records from second source").args(&[file_arg.clone(), records_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::table(&[])))
        .command(CommandSpec::named("prepend").description("Prepend records from second source").args(&[file_arg.clone(), records_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::table(&[])))
        .command(CommandSpec::named("merge").description("Merge fields row-by-row").args(&[file_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::table(&[])))
        .command(CommandSpec::named("join").description("SQL-style join on key field").args(&[
            file_arg.clone(),
            ArgSchemaInfo::string("left-key").required(),
            ArgSchemaInfo::string("right-key").optional(),
            ArgSchemaInfo::bool("left").flag("left"),
            ArgSchemaInfo::bool("right").flag("right"),
            ArgSchemaInfo::bool("outer").flag("outer"),
            ArgSchemaInfo::string("prefix").optional(),
            ArgSchemaInfo::string("suffix").optional(),
        ]).input(InputKind::Jsonl).output(OutputSpec::table(&[])))
        .command(CommandSpec::named("zip").description("Pair records element-by-element").args(&[file_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::table(&[])))
        .finish();
}
