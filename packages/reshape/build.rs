fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("reshape")
        .command(
            CommandSpec::named("rename")
                .description("Rename fields in records")
                .args(&[ArgSchemaInfo::string("column").required()])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("flatten")
                .description("Flatten nested record fields")
                .args(&[
                    ArgSchemaInfo::string("fields").optional(),
                    ArgSchemaInfo::bool("all").flag("all"),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("transpose")
                .description("Swap rows and columns")
                .args(&[ArgSchemaInfo::string("names").optional()])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("wrap")
                .description("Wrap each value into a named field")
                .args(&[ArgSchemaInfo::string("name").required()])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("group-by")
                .description("Group records by field value")
                .args(&[
                    ArgSchemaInfo::string("fields").required(),
                    ArgSchemaInfo::bool("to-table").flag("to-table"),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("enumerate")
                .description("Add zero-based index to records")
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("columns")
                .description("Extract field names from first record")
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[FieldSchemaInfo::string("value")])),
        )
        .command(
            CommandSpec::named("values")
                .description("Extract all values from records")
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[FieldSchemaInfo::string("value")])),
        )
        .finish();
}
