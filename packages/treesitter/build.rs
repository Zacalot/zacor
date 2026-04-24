fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("treesitter")
        .binary("zr-treesitter")
        .command(
            CommandSpec::named("parse")
                .description("Parse source code and return top-level declarations")
                .args(&[
                    ArgSchemaInfo::string("source").required(),
                    ArgSchemaInfo::string("ext").required(),
                    ArgSchemaInfo::string("rel_path").default(DefaultValue::String("")),
                ])
                .output(OutputSpec::table(&[
                    FieldSchemaInfo::string("file"),
                    FieldSchemaInfo::string("kind"),
                    FieldSchemaInfo::string("name"),
                    FieldSchemaInfo::string("signature"),
                ])),
        )
        .finish();
}
