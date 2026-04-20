fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("first")
        .command(
            CommandSpec::implicit_default()
                .description("Take first N records from stream")
                .args(&[ArgSchemaInfo::integer("count").optional().default(DefaultValue::Number(1))])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .finish();
}
