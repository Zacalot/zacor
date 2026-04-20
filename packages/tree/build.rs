fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("tree")
        .command(
            CommandSpec::implicit_default()
                .description("Show filesystem tree (respects .gitignore)")
                .args(&[
                    ArgSchemaInfo::path("path").default(DefaultValue::String(".")),
                    ArgSchemaInfo::integer("depth").flag("depth"),
                ])
                .output(OutputSpec::table(&[FieldSchemaInfo::string("line")])),
        )
        .finish();
}
