fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("zr-mermaid")
        .command(
            CommandSpec::named("render")
                .description("Render mermaid source to SVG")
                .args(&[ArgSchemaInfo::string("source").required()])
                .output(OutputSpec::record(&[FieldSchemaInfo::string("svg")])),
        )
        .finish();
}
