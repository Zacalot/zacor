fn main() {
    use zacor_package_build::*;
    PackageSpec::from_cargo("where")
        .command(
            CommandSpec::implicit_default()
                .description("Filter records by expression predicate")
                .args(&[ArgSchemaInfo::string("expr").required()])
                .input(InputKind::Jsonl)
                .output(OutputSpec::table(&[])),
        )
        .finish();
}
