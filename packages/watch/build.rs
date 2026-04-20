fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("watch")
        .command(
            CommandSpec::implicit_default()
                .description("Watch file system for changes")
                .args(&[
                    ArgSchemaInfo::path("path").default(DefaultValue::String(".")),
                    ArgSchemaInfo::bool("no-recursive").flag("R"),
                ])
                .output(OutputSpec::streaming_table(&[
                    FieldSchemaInfo::string("event"),
                    FieldSchemaInfo::string("path"),
                    FieldSchemaInfo::datetime("time"),
                ])),
        )
        .finish();
}
