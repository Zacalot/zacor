fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("ls")
        .command(
            CommandSpec::implicit_default()
                .description("List directory entries")
                .args(&[
                    ArgSchemaInfo::path("path").default(DefaultValue::String(".")),
                    ArgSchemaInfo::bool("all").flag("a"),
                ])
                .output(OutputSpec::table(&[
                    FieldSchemaInfo::string("name"),
                    FieldSchemaInfo::filesize("size"),
                    FieldSchemaInfo::string("kind"),
                ])),
        )
        .finish();
}
