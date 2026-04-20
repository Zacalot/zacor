fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("select")
        .command(
            CommandSpec::named("default")
                .description("Select fields from JSON input")
                .args(&[ArgSchemaInfo::string("fields").required()])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[
                    FieldSchemaInfo::string("value"),
                ])),
        )
        .command(
            CommandSpec::named("reject")
                .description("Remove specified fields from JSON input")
                .args(&[ArgSchemaInfo::string("fields").required()])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .finish();
}
