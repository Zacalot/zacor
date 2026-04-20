fn main() {
    use zacor_package_build::*;

    let search_output = [
        FieldSchemaInfo::string("engine"),
        FieldSchemaInfo::integer("rank"),
        FieldSchemaInfo::string("title"),
        FieldSchemaInfo::url("url"),
        FieldSchemaInfo::string("snippet"),
    ];

    let fetch_output = [
        FieldSchemaInfo::url("url"),
        FieldSchemaInfo::number("status"),
        FieldSchemaInfo::duration("elapsed"),
        FieldSchemaInfo::string("body"),
        FieldSchemaInfo::string("content_type"),
    ];

    PackageSpec::from_cargo("web")
        .command(
            CommandSpec::named("search")
                .description("Search the web")
                .args(&[
                    ArgSchemaInfo::string("query").required().rest(),
                    ArgSchemaInfo::string("engine")
                        .default(DefaultValue::String("duckduckgo"))
                        .flag("engine"),
                    ArgSchemaInfo::string("fallback")
                        .optional()
                        .flag("fallback"),
                    ArgSchemaInfo::integer("count")
                        .default(DefaultValue::Number(10))
                        .flag("count"),
                    ArgSchemaInfo::integer("page")
                        .default(DefaultValue::Number(1))
                        .flag("page"),
                    ArgSchemaInfo::integer("timeout")
                        .default(DefaultValue::Number(30))
                        .flag("timeout"),
                    ArgSchemaInfo::bool("news").flag("news"),
                ])
                .output(OutputSpec::table(&search_output)),
        )
        .command(
            CommandSpec::named("fetch")
                .description("Fetch a URL and output the response")
                .args(&[
                    ArgSchemaInfo::string("url").required(),
                    ArgSchemaInfo::integer("timeout")
                        .default(DefaultValue::Number(30))
                        .flag("timeout"),
                    ArgSchemaInfo::string("user-agent")
                        .default(DefaultValue::String("zr-web/0.1"))
                        .flag("user-agent"),
                ])
                .output(OutputSpec::record(&fetch_output)),
        )
        .command(
            CommandSpec::named("extract")
                .description("Reserved for future readable-content extraction")
                .args(&[]),
        )
        .finish();
}
