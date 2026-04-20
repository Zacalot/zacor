fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("filter")
        .command(
            CommandSpec::named("skip")
                .description("Skip the first N records")
                .args(&[ArgSchemaInfo::integer("count").required()])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("drop")
                .description("Drop the last N records")
                .args(&[ArgSchemaInfo::integer("count").optional().default(DefaultValue::Number(1))])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("uniq")
                .description("Remove consecutive duplicate records")
                .args(&[
                    ArgSchemaInfo::bool("count").flag("count"),
                    ArgSchemaInfo::bool("repeated").flag("repeated"),
                    ArgSchemaInfo::bool("unique").flag("unique"),
                    ArgSchemaInfo::bool("ignore-case").flag("ignore-case"),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("uniq-by")
                .description("Deduplicate records by field(s)")
                .args(&[
                    ArgSchemaInfo::string("fields").required(),
                    ArgSchemaInfo::bool("keep-last").flag("keep-last"),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("compact")
                .description("Remove records with null values")
                .args(&[
                    ArgSchemaInfo::string("fields").optional(),
                    ArgSchemaInfo::bool("empty").flag("empty"),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("find")
                .description("Search records for matching terms")
                .args(&[
                    ArgSchemaInfo::string("term").required(),
                    ArgSchemaInfo::bool("regex").flag("regex"),
                    ArgSchemaInfo::string("columns").optional(),
                    ArgSchemaInfo::bool("invert").flag("invert"),
                    ArgSchemaInfo::bool("ignore-case").flag("ignore-case"),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .finish();
}
