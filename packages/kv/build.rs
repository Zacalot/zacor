fn main() {
    use zacor_package_build::*;

    let kv_output = &[
        FieldSchemaInfo::string("key"),
        FieldSchemaInfo::string("value"),
    ];

    PackageSpec::from_cargo("kv")
        .execution_default("command")
        .project_data()
        .service("kv --listen=:{port}", 9200, "/health")
        .command(
            CommandSpec::named("set")
                .description("Store a key-value pair")
                .args(&[
                    ArgSchemaInfo::string("key").required(),
                    ArgSchemaInfo::string("value").required(),
                ])
                .output(OutputSpec::record(kv_output)),
        )
        .command(
            CommandSpec::named("get")
                .description("Retrieve a value by key")
                .args(&[ArgSchemaInfo::string("key").required()])
                .output(OutputSpec::record(kv_output)),
        )
        .command(
            CommandSpec::named("list")
                .description("List all key-value pairs")
                .output(OutputSpec::table(kv_output)),
        )
        .command(
            CommandSpec::named("delete")
                .description("Remove a key-value pair")
                .args(&[ArgSchemaInfo::string("key").required()])
                .output(OutputSpec::record(kv_output)),
        )
        .finish();
}
