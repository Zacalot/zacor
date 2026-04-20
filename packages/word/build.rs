fn main() {
    use zacor_package_build::*;

    let lookup_fields = [
        FieldSchemaInfo::string("value"),
        FieldSchemaInfo::string("pos"),
        FieldSchemaInfo::string("domain"),
        FieldSchemaInfo::string("definition"),
        FieldSchemaInfo::string("examples"),
        FieldSchemaInfo::number("frequency"),
        FieldSchemaInfo::number("sense"),
    ];

    let related_fields = [
        FieldSchemaInfo::string("value"),
        FieldSchemaInfo::string("relation"),
        FieldSchemaInfo::string("definition"),
        FieldSchemaInfo::string("pos"),
        FieldSchemaInfo::number("depth"),
    ];

    let domain_fields = [
        FieldSchemaInfo::string("value"),
        FieldSchemaInfo::string("definition"),
        FieldSchemaInfo::string("pos"),
        FieldSchemaInfo::number("count"),
    ];

    let random_fields = [
        FieldSchemaInfo::string("value"),
        FieldSchemaInfo::string("pos"),
        FieldSchemaInfo::string("domain"),
        FieldSchemaInfo::string("definition"),
    ];

    let pattern_fields = [
        FieldSchemaInfo::string("value"),
        FieldSchemaInfo::string("pos"),
        FieldSchemaInfo::string("definition"),
    ];

    let sentence_fields = [
        FieldSchemaInfo::string("value"),
        FieldSchemaInfo::string("template"),
    ];

    PackageSpec::from_cargo("word")
        .command(
            CommandSpec::named("lookup")
                .description("Look up word definitions and senses")
                .args(&[
                    ArgSchemaInfo::string("word").required(),
                    ArgSchemaInfo::string("pos").optional().flag("pos"),
                ])
                .output(OutputSpec::streaming_table(&lookup_fields)),
        )
        .command(
            CommandSpec::named("related")
                .description("Find semantically related words")
                .args(&[
                    ArgSchemaInfo::string("word").required(),
                    ArgSchemaInfo::string("relation").optional().flag("relation"),
                    ArgSchemaInfo::integer("depth").optional().flag("depth"),
                    ArgSchemaInfo::string("pos").optional().flag("pos"),
                    ArgSchemaInfo::integer("sense").optional().flag("sense"),
                ])
                .output(OutputSpec::streaming_table(&related_fields)),
        )
        .command(
            CommandSpec::named("domain")
                .description("Browse words by semantic domain")
                .args(&[
                    ArgSchemaInfo::string("domain").optional(),
                    ArgSchemaInfo::integer("count").optional().flag("count"),
                ])
                .output(OutputSpec::streaming_table(&domain_fields)),
        )
        .command(
            CommandSpec::named("random")
                .description("Pick random words from vocabulary")
                .args(&[
                    ArgSchemaInfo::string("pos").optional().flag("pos"),
                    ArgSchemaInfo::string("domain").optional().flag("domain"),
                    ArgSchemaInfo::integer("count").optional().flag("count"),
                    ArgSchemaInfo::number("seed").optional().flag("seed"),
                ])
                .output(OutputSpec::streaming_table(&random_fields)),
        )
        .command(
            CommandSpec::named("pattern")
                .description("Find words matching a wildcard pattern")
                .args(&[
                    ArgSchemaInfo::string("pattern").required(),
                    ArgSchemaInfo::string("pos").optional().flag("pos"),
                    ArgSchemaInfo::integer("count").optional().flag("count"),
                ])
                .output(OutputSpec::streaming_table(&pattern_fields)),
        )
        .command(
            CommandSpec::named("sentence")
                .description("Generate random sentences from POS templates")
                .args(&[
                    ArgSchemaInfo::string("template").optional().flag("template"),
                    ArgSchemaInfo::bool("raw").flag("raw"),
                    ArgSchemaInfo::integer("count").optional().flag("count"),
                    ArgSchemaInfo::number("seed").optional().flag("seed"),
                ])
                .output(OutputSpec::streaming_table(&sentence_fields)),
        )
        .finish();
}
