fn main() {
    use zacor_package_build::*;

    let field_arg = ArgSchemaInfo::string("field").optional();

    PackageSpec::from_cargo("math")
        .command(CommandSpec::named("sum").description("Sum numeric values").args(&[field_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::record(&[FieldSchemaInfo::number("value")])))
        .command(CommandSpec::named("avg").description("Average numeric values").args(&[field_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::record(&[FieldSchemaInfo::number("value")])))
        .command(CommandSpec::named("min").description("Minimum value").args(&[field_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::record(&[FieldSchemaInfo::number("value")])))
        .command(CommandSpec::named("max").description("Maximum value").args(&[field_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::record(&[FieldSchemaInfo::number("value")])))
        .command(CommandSpec::named("median").description("Median value").args(&[field_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::record(&[FieldSchemaInfo::number("value")])))
        .command(CommandSpec::named("mode").description("Most frequent value").args(&[field_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::record(&[FieldSchemaInfo::number("value")])))
        .command(CommandSpec::named("product").description("Product of values").args(&[field_arg.clone()]).input(InputKind::Jsonl).output(OutputSpec::record(&[FieldSchemaInfo::number("value")])))
        .command(CommandSpec::named("stddev").description("Standard deviation").args(&[field_arg.clone(), ArgSchemaInfo::bool("sample").flag("sample")]).input(InputKind::Jsonl).output(OutputSpec::record(&[FieldSchemaInfo::number("value")])))
        .command(CommandSpec::named("variance").description("Variance").args(&[field_arg.clone(), ArgSchemaInfo::bool("sample").flag("sample")]).input(InputKind::Jsonl).output(OutputSpec::record(&[FieldSchemaInfo::number("value")])))
        .command(CommandSpec::named("count").description("Count records").input(InputKind::Jsonl).output(OutputSpec::record(&[FieldSchemaInfo::integer("count")])))
        .command(CommandSpec::named("round").description("Round numeric values").args(&[ArgSchemaInfo::string("field").required(), ArgSchemaInfo::integer("precision").optional().default(DefaultValue::Number(0))]).input(InputKind::Jsonl).output(OutputSpec::table(&[])))
        .command(CommandSpec::named("ceil").description("Ceiling of numeric values").args(&[ArgSchemaInfo::string("field").required()]).input(InputKind::Jsonl).output(OutputSpec::table(&[])))
        .command(CommandSpec::named("floor").description("Floor of numeric values").args(&[ArgSchemaInfo::string("field").required()]).input(InputKind::Jsonl).output(OutputSpec::table(&[])))
        .command(CommandSpec::named("abs").description("Absolute value").args(&[ArgSchemaInfo::string("field").required()]).input(InputKind::Jsonl).output(OutputSpec::table(&[])))
        .finish();
}
