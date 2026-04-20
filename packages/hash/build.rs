fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("hash")
        .command(
            CommandSpec::implicit_default()
                .description("Compute cryptographic hash of files or stdin")
                .args(&[
                    ArgSchemaInfo::path("file").optional(),
                    ArgSchemaInfo::string("algorithm").default(DefaultValue::String("sha256")),
                ])
                .input(InputKind::Text)
                .output(OutputSpec::table(&[
                    FieldSchemaInfo::string("hash"),
                    FieldSchemaInfo::string("algorithm"),
                    FieldSchemaInfo::string("file"),
                ])),
        )
        .finish();
}
