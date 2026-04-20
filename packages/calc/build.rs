fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("calc")
        .command(
            CommandSpec::implicit_default()
                .description("Evaluate a math expression")
                .args(&[ArgSchemaInfo::string("expr")])
                .output(OutputSpec::infer(&[FieldSchemaInfo::number("value")])),
        )
        .finish();
}
