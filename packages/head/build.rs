fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("head")
        .command(
            CommandSpec::implicit_default()
                .description("Show first N lines")
                .args(&[
                    ArgSchemaInfo::path("file").optional(),
                    ArgSchemaInfo::integer("lines").default(DefaultValue::Number(10)),
                ])
                .input(InputKind::Text)
                .output(OutputSpec::streaming_table(&[
                    FieldSchemaInfo::number("line"),
                    FieldSchemaInfo::string("content"),
                ])),
        )
        .finish();
}
