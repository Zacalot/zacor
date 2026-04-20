fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("wc")
        .command(
            CommandSpec::implicit_default()
                .description("Count file statistics")
                .args(&[ArgSchemaInfo::path("file").optional()])
                .input(InputKind::Text)
                .output(OutputSpec::record(&[
                    FieldSchemaInfo::string("file"),
                    FieldSchemaInfo::number("lines"),
                    FieldSchemaInfo::number("words"),
                    FieldSchemaInfo::number("bytes"),
                ])),
        )
        .finish();
}
