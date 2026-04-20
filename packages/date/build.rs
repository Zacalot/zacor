fn main() {
    use zacor_package_build::*;

    let date_record_fields = [
        FieldSchemaInfo::datetime("datetime"),
        FieldSchemaInfo::string("date"),
        FieldSchemaInfo::string("time"),
        FieldSchemaInfo::number("year"),
        FieldSchemaInfo::number("month"),
        FieldSchemaInfo::number("day"),
        FieldSchemaInfo::number("hour"),
        FieldSchemaInfo::number("minute"),
        FieldSchemaInfo::number("second"),
        FieldSchemaInfo::number("nanosecond"),
        FieldSchemaInfo::string("weekday"),
        FieldSchemaInfo::number("weekday_num"),
        FieldSchemaInfo::number("week"),
        FieldSchemaInfo::number("day_of_year"),
        FieldSchemaInfo::number("quarter"),
        FieldSchemaInfo::string("timezone"),
        FieldSchemaInfo::string("offset"),
        FieldSchemaInfo::number("unix"),
        FieldSchemaInfo::number("unix_ms"),
        FieldSchemaInfo::string("iso8601"),
        FieldSchemaInfo::string("rfc2822"),
        FieldSchemaInfo::string("rfc9557"),
        FieldSchemaInfo::bool("is_dst"),
        FieldSchemaInfo::bool("is_leap_year"),
        FieldSchemaInfo::number("days_in_month"),
        FieldSchemaInfo::number("days_in_year"),
    ];

    let diff_record_fields = [
        FieldSchemaInfo::datetime("from"),
        FieldSchemaInfo::datetime("to"),
        FieldSchemaInfo::number("years"),
        FieldSchemaInfo::number("months"),
        FieldSchemaInfo::number("weeks"),
        FieldSchemaInfo::number("days"),
        FieldSchemaInfo::number("hours"),
        FieldSchemaInfo::number("minutes"),
        FieldSchemaInfo::number("seconds"),
        FieldSchemaInfo::number("total_days"),
        FieldSchemaInfo::number("total_hours"),
        FieldSchemaInfo::number("total_seconds"),
        FieldSchemaInfo::string("humanized"),
        FieldSchemaInfo::string("iso8601"),
    ];

    let zone_record_fields = [
        FieldSchemaInfo::string("name"),
        FieldSchemaInfo::string("offset"),
        FieldSchemaInfo::string("abbreviation"),
        FieldSchemaInfo::bool("is_dst"),
    ];

    PackageSpec::from_cargo("date")
        .command(
            CommandSpec::named("default")
                .description("Display current datetime or parse a date string")
                .args(&[
                    ArgSchemaInfo::string("date").optional(),
                    ArgSchemaInfo::string("timezone").optional(),
                    ArgSchemaInfo::bool("utc").flag("utc"),
                ])
                .output(OutputSpec::record(&date_record_fields)),
        )
        .command(
            CommandSpec::named("add")
                .description("Add a duration to a date")
                .args(&[
                    ArgSchemaInfo::string("date").optional(),
                    ArgSchemaInfo::string("duration").required(),
                    ArgSchemaInfo::string("timezone").optional(),
                    ArgSchemaInfo::bool("utc").flag("utc"),
                ])
                .output(OutputSpec::record(&date_record_fields)),
        )
        .command(
            CommandSpec::named("diff")
                .description("Compute the span between two dates")
                .args(&[
                    ArgSchemaInfo::string("from").required(),
                    ArgSchemaInfo::string("to").optional(),
                ])
                .output(OutputSpec::record(&diff_record_fields)),
        )
        .command(
            CommandSpec::named("seq")
                .description("Generate a sequence of dates")
                .args(&[
                    ArgSchemaInfo::string("from").optional(),
                    ArgSchemaInfo::string("to").optional(),
                    ArgSchemaInfo::number("count").optional(),
                    ArgSchemaInfo::string("step").optional(),
                    ArgSchemaInfo::string("timezone").optional(),
                    ArgSchemaInfo::bool("utc").flag("utc"),
                ])
                .output(OutputSpec::streaming_table(&date_record_fields)),
        )
        .command(
            CommandSpec::named("round")
                .description("Round a datetime to the nearest unit")
                .args(&[
                    ArgSchemaInfo::string("date").optional(),
                    ArgSchemaInfo::string("to").required(),
                    ArgSchemaInfo::string("timezone").optional(),
                    ArgSchemaInfo::bool("utc").flag("utc"),
                ])
                .output(OutputSpec::record(&date_record_fields)),
        )
        .command(
            CommandSpec::named("zones")
                .description("List all IANA timezones")
                .args(&[])
                .output(OutputSpec::streaming_table(&zone_record_fields)),
        )
        .finish();
}
