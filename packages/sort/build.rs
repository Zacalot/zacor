fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("sort")
        .command(
            CommandSpec::named("by")
                .description("Sort records by field(s)")
                .args(&[
                    ArgSchemaInfo::string("fields").required(),
                    ArgSchemaInfo::bool("reverse").flag("reverse"),
                    ArgSchemaInfo::bool("natural").flag("natural"),
                    ArgSchemaInfo::bool("ignore-case").flag("ignore-case"),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("reverse")
                .description("Reverse the order of records")
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .finish();
}
