fn main() {
    use zacor_package_build::*;

    let value_number = [FieldSchemaInfo::number("value")];
    let value_float = [FieldSchemaInfo::number("value")];
    let value_bool = [FieldSchemaInfo::bool("value")];
    let value_string = [FieldSchemaInfo::string("value")];
    let color_fields = [
        FieldSchemaInfo::string("value"),
        FieldSchemaInfo::number("r"),
        FieldSchemaInfo::number("g"),
        FieldSchemaInfo::number("b"),
    ];
    let name_full_fields = [
        FieldSchemaInfo::string("value"),
        FieldSchemaInfo::string("first"),
        FieldSchemaInfo::string("last"),
    ];
    let archetype_fields = [
        FieldSchemaInfo::string("value"),
        FieldSchemaInfo::string("kind"),
        FieldSchemaInfo::string("tags"),
        FieldSchemaInfo::string("role"),
        FieldSchemaInfo::string("label"),
        FieldSchemaInfo::string("traits"),
    ];
    let motive_fields = [
        FieldSchemaInfo::string("value"),
        FieldSchemaInfo::string("kind"),
        FieldSchemaInfo::string("tags"),
        FieldSchemaInfo::string("drive"),
        FieldSchemaInfo::string("goal"),
        FieldSchemaInfo::string("obstacle"),
        FieldSchemaInfo::string("outcome"),
        FieldSchemaInfo::string("forces"),
    ];

    let count_arg = ArgSchemaInfo::integer("count").optional();
    let seed_arg = ArgSchemaInfo::number("seed").optional();

    PackageSpec::from_cargo("rand")
        .command(
            CommandSpec::named("int")
                .description("Generate random integers")
                .args(&[
                    ArgSchemaInfo::integer("min").optional(),
                    ArgSchemaInfo::integer("max").optional(),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .output(OutputSpec::streaming_table(&value_number)),
        )
        .command(
            CommandSpec::named("float")
                .description("Generate random floats")
                .args(&[
                    ArgSchemaInfo::number("min").optional(),
                    ArgSchemaInfo::number("max").optional(),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .output(OutputSpec::streaming_table(&value_float)),
        )
        .command(
            CommandSpec::named("bool")
                .description("Generate random booleans")
                .args(&[count_arg.clone(), seed_arg.clone()])
                .output(OutputSpec::streaming_table(&value_bool)),
        )
        .command(
            CommandSpec::named("word")
                .description("Pick random words from dictionary")
                .args(&[
                    ArgSchemaInfo::string("pool").optional(),
                    ArgSchemaInfo::string("locale").optional(),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .output(OutputSpec::streaming_table(&value_string)),
        )
        .command(
            CommandSpec::named("syllable")
                .description("Generate invented words from syllables")
                .args(&[
                    ArgSchemaInfo::string("set").optional(),
                    ArgSchemaInfo::integer("min_syllables").optional(),
                    ArgSchemaInfo::integer("max_syllables").optional(),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .output(OutputSpec::streaming_table(&value_string)),
        )
        .command(
            CommandSpec::named("name")
                .description("Generate random person names")
                .args(&[
                    ArgSchemaInfo::string("kind").optional(),
                    ArgSchemaInfo::string("pool").optional(),
                    ArgSchemaInfo::string("locale").optional(),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .output(OutputSpec::streaming_table(&name_full_fields)),
        )
        .command(
            CommandSpec::named("char")
                .description("Generate random character strings")
                .args(&[
                    ArgSchemaInfo::integer("len").optional(),
                    ArgSchemaInfo::string("charset").optional(),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .output(OutputSpec::streaming_table(&value_string)),
        )
        .command(
            CommandSpec::named("uuid")
                .description("Generate random UUID v4")
                .args(&[count_arg.clone(), seed_arg.clone()])
                .output(OutputSpec::streaming_table(&value_string)),
        )
        .command(
            CommandSpec::named("phrase")
                .description("Generate random word phrases")
                .args(&[
                    ArgSchemaInfo::integer("words").optional(),
                    ArgSchemaInfo::string("sep").optional(),
                    ArgSchemaInfo::string("pool").optional(),
                    ArgSchemaInfo::string("locale").optional(),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .output(OutputSpec::streaming_table(&value_string)),
        )
        .command(
            CommandSpec::named("pass")
                .description("Generate random passwords")
                .args(&[
                    ArgSchemaInfo::integer("len").optional(),
                    ArgSchemaInfo::bool("upper").flag("upper"),
                    ArgSchemaInfo::bool("lower").flag("lower"),
                    ArgSchemaInfo::bool("digit").flag("digit"),
                    ArgSchemaInfo::bool("symbol").flag("symbol"),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .output(OutputSpec::streaming_table(&value_string)),
        )
        .command(
            CommandSpec::named("pattern")
                .description("Generate strings from format pattern")
                .args(&[
                    ArgSchemaInfo::string("fmt").required(),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .output(OutputSpec::streaming_table(&value_string)),
        )
        .command(
            CommandSpec::named("color")
                .description("Generate random colors")
                .args(&[count_arg.clone(), seed_arg.clone()])
                .output(OutputSpec::streaming_table(&color_fields)),
        )
        .command(
            CommandSpec::named("date")
                .description("Generate random dates")
                .args(&[
                    ArgSchemaInfo::string("min").optional(),
                    ArgSchemaInfo::string("max").optional(),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .output(OutputSpec::streaming_table(&value_string)),
        )
        .command(
            CommandSpec::named("pick")
                .description("Pick random items from input, values, or file")
                .args(&[
                    ArgSchemaInfo::string("values").optional(),
                    ArgSchemaInfo::string("file").optional(),
                    ArgSchemaInfo::bool("replace").flag("replace"),
                    count_arg.clone(),
                    seed_arg.clone(),
                ])
                .input(InputKind::Jsonl)
                .output(OutputSpec::streaming_table(&value_string)),
        )
        .command(
            CommandSpec::named("shuffle")
                .description("Shuffle input records into random order")
                .args(&[seed_arg.clone()])
                .input(InputKind::Jsonl)
                .output(OutputSpec::streaming_table(&value_string)),
        )
        .command(
            CommandSpec::named("character")
                .description("Generate higher-level character ideas")
                .subcommand(
                    CommandSpec::named("archetype")
                        .description("Generate random character archetypes")
                        .args(&[
                            ArgSchemaInfo::string("include").optional(),
                            ArgSchemaInfo::string("exclude").optional(),
                            count_arg.clone(),
                            seed_arg.clone(),
                        ])
                        .output(OutputSpec::streaming_table(&archetype_fields)),
                )
                .subcommand(
                    CommandSpec::named("motive")
                        .description("Generate random character motives")
                        .args(&[
                            ArgSchemaInfo::string("include").optional(),
                            ArgSchemaInfo::string("exclude").optional(),
                            count_arg.clone(),
                            seed_arg.clone(),
                        ])
                        .output(OutputSpec::streaming_table(&motive_fields)),
                ),
        )
        .finish();
}
