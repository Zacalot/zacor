fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("cat")
        .command(
            CommandSpec::implicit_default()
                .description("Output file contents as line records")
                .args(&[
                    ArgSchemaInfo::path("file").optional(),
                    ArgSchemaInfo::integer("lines").optional(),
                    ArgSchemaInfo::integer("tail").optional(),
                ])
                .input(InputKind::Text)
                .output(OutputSpec::streaming_table(&[
                    FieldSchemaInfo::number("line"),
                    FieldSchemaInfo::string("content"),
                ])),
        )
        .finish();
}
