fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("echo")
        .command(
            CommandSpec::implicit_default()
                .description("Echo text to stdout")
                .args(&[ArgSchemaInfo::string("text").default(DefaultValue::String(""))])
                .output(OutputSpec::infer(&[FieldSchemaInfo::string("text")])),
        )
        .finish();
}
