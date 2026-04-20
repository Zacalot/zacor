fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("mutate")
        .command(
            CommandSpec::named("insert")
                .description("Add a new field to each record")
                .args(&[
                    ArgSchemaInfo::string("field").required(),
                    ArgSchemaInfo::string("value").optional(),
                    ArgSchemaInfo::string("expr").optional(),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("update")
                .description("Update an existing field in each record")
                .args(&[
                    ArgSchemaInfo::string("field").required(),
                    ArgSchemaInfo::string("value").optional(),
                    ArgSchemaInfo::string("expr").optional(),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .command(
            CommandSpec::named("upsert")
                .description("Add or update a field in each record")
                .args(&[
                    ArgSchemaInfo::string("field").required(),
                    ArgSchemaInfo::string("value").optional(),
                    ArgSchemaInfo::string("expr").optional(),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .finish();
}
