fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("json")
        .command(
            CommandSpec::implicit_default()
                .description("Format or validate JSON")
                .args(&[
                    ArgSchemaInfo::integer("indent").default(DefaultValue::Number(2)),
                    ArgSchemaInfo::bool("compact").flag("compact"),
                    ArgSchemaInfo::bool("validate").flag("validate"),
                ])
                .input(InputKind::Text)
                .output(OutputSpec::text(
                    "output",
                    &[
                        FieldSchemaInfo::string("output"),
                        FieldSchemaInfo::bool("valid"),
                    ],
                )),
        )
        .finish();
}
