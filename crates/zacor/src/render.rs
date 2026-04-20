use crate::package_definition::{OutputDeclaration, OutputType};
use comfy_table::{ContentArrangement, Table, presets::NOTHING};
use humansize::{BINARY, FormatSizeOptions, format_size};
use serde_json::Value;
use std::io::{BufRead, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Rich,
    Plain,
}

// ─── Semantic Type Formatters ───────────────────────────────────────

pub fn format_value(value: &Value, semantic_type: &str) -> String {
    match semantic_type {
        "filesize" => format_filesize(value),
        "datetime" => format_datetime(value),
        "duration" => format_duration(value),
        _ => value_to_string(value),
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        // Nested objects/arrays → compact JSON
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn filesize_opts() -> FormatSizeOptions {
    BINARY.decimal_places(1)
}

fn format_filesize(value: &Value) -> String {
    let bytes = match value {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        _ => return value_to_string(value),
    };
    format_size(bytes as u64, filesize_opts())
}

fn format_datetime(value: &Value) -> String {
    let s = match value {
        Value::String(s) => s.as_str(),
        _ => return value_to_string(value),
    };
    let parsed = match chrono::DateTime::parse_from_rfc3339(s) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => return s.to_string(),
    };
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(parsed);

    if duration.num_seconds() < 0 {
        return s.to_string();
    }

    let mut f = timeago::Formatter::new();
    f.min_unit(timeago::TimeUnit::Minutes).too_low("just now");
    f.convert(duration.to_std().unwrap_or_default())
}

fn format_duration(value: &Value) -> String {
    let secs = match value {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        _ => return value_to_string(value),
    };

    if secs < 60.0 {
        // Strip trailing zeros but keep one decimal
        let formatted = format!("{:.1}s", secs);
        return formatted;
    }

    let total_secs = secs;
    let mins = (total_secs / 60.0).floor() as u64;
    let remaining = total_secs - (mins as f64 * 60.0);
    format!("{}m {:.1}s", mins, remaining)
}

// ─── Render Functions ───────────────────────────────────────────────

fn get_schema_fields(output: &OutputDeclaration) -> Vec<(String, String)> {
    output
        .schema
        .as_ref()
        .map(|s| {
            s.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn extract_field_value(record: &Value, field: &str, semantic_type: &str) -> String {
    match record.get(field) {
        Some(v) => format_value(v, semantic_type),
        None => String::new(),
    }
}

fn record_text_value(record: &Value, output: &OutputDeclaration) -> String {
    if let Some(ref field) = output.field {
        match record.get(field) {
            Some(Value::String(s)) => s.clone(),
            Some(v) => value_to_string(v),
            None => String::new(),
        }
    } else {
        let vals: Vec<String> = match record.as_object() {
            Some(obj) => obj.values().map(value_to_string).collect(),
            None => vec![value_to_string(record)],
        };
        vals.join(" ")
    }
}

fn plain_row_values(record: &Value, output: &OutputDeclaration) -> Vec<String> {
    let fields = get_schema_fields(output);
    if fields.is_empty() {
        match record.as_object() {
            Some(obj) => obj.values().map(value_to_string).collect(),
            None => vec![value_to_string(record)],
        }
    } else {
        fields
            .iter()
            .map(|(field_name, semantic_type)| {
                extract_field_value(record, field_name, semantic_type)
            })
            .collect()
    }
}

pub fn render_text(records: &[Value], output: &OutputDeclaration, writer: &mut impl Write) {
    for record in records {
        let _ = writeln!(writer, "{}", record_text_value(record, output));
    }
}

pub fn render_plain_record(record: &Value, output: &OutputDeclaration, writer: &mut impl Write) {
    let _ = writeln!(writer, "{}", plain_row_values(record, output).join("\t"));
}

pub fn render_plain_table(records: &[Value], output: &OutputDeclaration, writer: &mut impl Write) {
    for record in records {
        let _ = writeln!(writer, "{}", plain_row_values(record, output).join("\t"));
    }
}

fn make_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(NOTHING)
        .set_content_arrangement(ContentArrangement::Disabled);
    table
}

pub fn render_record(record: &Value, output: &OutputDeclaration, writer: &mut impl Write) {
    let fields = get_schema_fields(output);

    let mut table = make_table();

    if fields.is_empty() {
        if let Some(obj) = record.as_object() {
            for (k, v) in obj {
                table.add_row(vec![k.as_str(), &value_to_string(v)]);
            }
        }
    } else {
        for (field_name, semantic_type) in &fields {
            let value = extract_field_value(record, field_name, semantic_type);
            table.add_row(vec![field_name.as_str(), &value]);
        }
    }

    if table.row_count() > 0 {
        let _ = writeln!(writer, "{}", table);
    }
}

pub fn render_table(records: &[Value], output: &OutputDeclaration, writer: &mut impl Write) {
    if records.is_empty() {
        return;
    }

    let fields = get_schema_fields(output);
    if fields.is_empty() {
        return;
    }

    let mut table = make_table();

    // Header
    let headers: Vec<String> = fields.iter().map(|(k, _)| k.to_uppercase()).collect();
    table.set_header(&headers);

    // Rows
    for record in records {
        let row: Vec<String> = fields
            .iter()
            .map(|(field_name, semantic_type)| {
                extract_field_value(record, field_name, semantic_type)
            })
            .collect();
        table.add_row(&row);
    }

    let _ = writeln!(writer, "{}", table);
}

pub fn render_batch(
    records: &[Value],
    output: &OutputDeclaration,
    mode: RenderMode,
    writer: &mut impl Write,
) {
    match mode {
        RenderMode::Rich => match output.resolved_output_type() {
            OutputType::Text => render_text(records, output, writer),
            OutputType::Record => {
                if let Some(record) = records.first() {
                    render_record(record, output, writer);
                }
            }
            OutputType::Table => render_table(records, output, writer),
        },
        RenderMode::Plain => match output.resolved_output_type() {
            OutputType::Text => render_text(records, output, writer),
            OutputType::Record => {
                if let Some(record) = records.first() {
                    render_plain_record(record, output, writer);
                }
            }
            OutputType::Table => render_plain_table(records, output, writer),
        },
    }
}

// ─── Streaming Rendering ──────────────────────────────────────────────

/// Print the table header for streaming output. Called once before any rows.
pub fn render_streaming_header(
    output: &OutputDeclaration,
    mode: RenderMode,
    writer: &mut impl Write,
) {
    if mode == RenderMode::Rich && output.resolved_output_type() == OutputType::Table {
        let fields = get_schema_fields(output);
        if !fields.is_empty() {
            let headers: Vec<String> = fields.iter().map(|(k, _)| k.to_uppercase()).collect();
            let _ = writeln!(writer, "{}", headers.join("\t"));
        }
    }
}

/// Print a single row for streaming output. Called for each OUTPUT message.
pub fn render_streaming_row(
    record: &Value,
    output: &OutputDeclaration,
    mode: RenderMode,
    writer: &mut impl Write,
) {
    match mode {
        RenderMode::Rich => match output.resolved_output_type() {
            OutputType::Text => {
                let _ = writeln!(writer, "{}", record_text_value(record, output));
            }
            OutputType::Record => {
                render_record(record, output, writer);
            }
            OutputType::Table => {
                let _ = writeln!(writer, "{}", plain_row_values(record, output).join("\t"));
            }
        },
        RenderMode::Plain => match output.resolved_output_type() {
            OutputType::Text => {
                let _ = writeln!(writer, "{}", record_text_value(record, output));
            }
            OutputType::Record => {
                render_plain_record(record, output, writer);
            }
            OutputType::Table => {
                let _ = writeln!(writer, "{}", plain_row_values(record, output).join("\t"));
            }
        },
    }
    let _ = writer.flush();
}

// ─── Main Entry Point ───────────────────────────────────────────────

pub fn render_jsonl(
    reader: impl BufRead,
    output: &OutputDeclaration,
    mode: RenderMode,
    mut writer: impl Write,
) {
    let records: Vec<Value> = reader
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect();
    render_batch(&records, output, mode, &mut writer);
    let _ = writer.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_output(
        output_type: OutputType,
        field: Option<&str>,
        stream: bool,
        schema: Vec<(&str, &str)>,
    ) -> OutputDeclaration {
        OutputDeclaration {
            output_type: Some(output_type),
            cardinality: None,
            display: None,
            field: field.map(|s| s.to_string()),
            stream,
            schema: if schema.is_empty() {
                None
            } else {
                Some(
                    schema
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect::<BTreeMap<_, _>>(),
                )
            },
        }
    }

    // ─── Formatter tests ────────────────────────────────────────────

    #[test]
    fn test_format_filesize_bytes() {
        assert_eq!(format_value(&serde_json::json!(0), "filesize"), "0 B");
        assert_eq!(format_value(&serde_json::json!(512), "filesize"), "512 B");
    }

    #[test]
    fn test_format_filesize_kib() {
        assert_eq!(format_value(&serde_json::json!(1024), "filesize"), "1 KiB");
        assert_eq!(
            format_value(&serde_json::json!(1536), "filesize"),
            "1.5 KiB"
        );
    }

    #[test]
    fn test_format_filesize_mib() {
        assert_eq!(
            format_value(&serde_json::json!(15728640), "filesize"),
            "15 MiB"
        );
    }

    #[test]
    fn test_format_filesize_gib() {
        assert_eq!(
            format_value(&serde_json::json!(1073741824), "filesize"),
            "1 GiB"
        );
    }

    #[test]
    fn test_format_filesize_tib() {
        assert_eq!(
            format_value(&serde_json::json!(1099511627776_u64), "filesize"),
            "1 TiB"
        );
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_value(&serde_json::json!(5.3), "duration"), "5.3s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(
            format_value(&serde_json::json!(125.7), "duration"),
            "2m 5.7s"
        );
    }

    #[test]
    fn test_format_duration_zero() {
        assert_eq!(format_value(&serde_json::json!(0), "duration"), "0.0s");
    }

    #[test]
    fn test_format_passthrough_string() {
        assert_eq!(format_value(&serde_json::json!("hello"), "string"), "hello");
    }

    #[test]
    fn test_format_passthrough_number() {
        assert_eq!(format_value(&serde_json::json!(42), "number"), "42");
    }

    #[test]
    fn test_format_passthrough_bool() {
        assert_eq!(format_value(&serde_json::json!(true), "bool"), "true");
    }

    #[test]
    fn test_format_passthrough_path() {
        assert_eq!(
            format_value(&serde_json::json!("/usr/bin"), "path"),
            "/usr/bin"
        );
    }

    #[test]
    fn test_format_passthrough_url() {
        assert_eq!(
            format_value(&serde_json::json!("https://example.com"), "url"),
            "https://example.com"
        );
    }

    #[test]
    fn test_format_unknown_type_passthrough() {
        assert_eq!(
            format_value(&serde_json::json!("data"), "custom-type"),
            "data"
        );
    }

    #[test]
    fn test_format_nested_object() {
        let val = serde_json::json!({"author": "bar"});
        assert_eq!(format_value(&val, "string"), r#"{"author":"bar"}"#);
    }

    #[test]
    fn test_format_nested_array() {
        let val = serde_json::json!([1, 2, 3]);
        assert_eq!(format_value(&val, "string"), "[1,2,3]");
    }

    // ─── Text rendering tests ───────────────────────────────────────

    #[test]
    fn test_render_text_with_field() {
        let output = make_output(
            OutputType::Text,
            Some("text"),
            false,
            vec![("text", "string")],
        );
        let records = vec![serde_json::json!({"text": "hello world"})];
        let mut buf = Vec::new();
        render_text(&records, &output, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "hello world\n");
    }

    #[test]
    fn test_render_text_multi_record() {
        let output = make_output(
            OutputType::Text,
            Some("match"),
            false,
            vec![("match", "string")],
        );
        let records = vec![
            serde_json::json!({"match": "line one"}),
            serde_json::json!({"match": "line two"}),
        ];
        let mut buf = Vec::new();
        render_text(&records, &output, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "line one\nline two\n");
    }

    #[test]
    fn test_render_text_no_field() {
        let output = make_output(OutputType::Text, None, false, vec![]);
        let records = vec![serde_json::json!({"a": "hello", "b": "world"})];
        let mut buf = Vec::new();
        render_text(&records, &output, &mut buf);
        let result = String::from_utf8(buf).unwrap();
        // Values printed space-separated (BTreeMap order: a, b)
        assert_eq!(result, "hello world\n");
    }

    #[test]
    fn test_render_plain_record_with_schema() {
        let output = make_output(
            OutputType::Record,
            None,
            false,
            vec![("file", "string"), ("size", "filesize")],
        );
        let record = serde_json::json!({"file": "foo.txt", "size": 1234});
        let mut buf = Vec::new();
        render_plain_record(&record, &output, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "foo.txt\t1.2 KiB\n");
    }

    #[test]
    fn test_render_plain_table_omits_header() {
        let output = make_output(
            OutputType::Table,
            None,
            false,
            vec![("name", "string"), ("kind", "string")],
        );
        let records = vec![
            serde_json::json!({"name": "a", "kind": "file"}),
            serde_json::json!({"name": "b", "kind": "dir"}),
        ];
        let mut buf = Vec::new();
        render_plain_table(&records, &output, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "file\ta\ndir\tb\n");
    }

    // ─── Record rendering tests ─────────────────────────────────────

    #[test]
    fn test_render_record_basic() {
        let output = make_output(
            OutputType::Record,
            None,
            false,
            vec![
                ("file", "string"),
                ("lines", "number"),
                ("words", "number"),
                ("bytes", "number"),
            ],
        );
        let record =
            serde_json::json!({"file": "test.txt", "lines": 10, "words": 50, "bytes": 1234});
        let mut buf = Vec::new();
        render_record(&record, &output, &mut buf);
        let result = String::from_utf8(buf).unwrap();
        assert!(result.contains("bytes"));
        assert!(result.contains("1234"));
        assert!(result.contains("file"));
        assert!(result.contains("test.txt"));
    }

    #[test]
    fn test_render_record_with_filesize() {
        let output = make_output(
            OutputType::Record,
            None,
            false,
            vec![("hash", "string"), ("size", "filesize")],
        );
        let record = serde_json::json!({"hash": "abc123", "size": 1234});
        let mut buf = Vec::new();
        render_record(&record, &output, &mut buf);
        let result = String::from_utf8(buf).unwrap();
        assert!(result.contains("1.2 KiB"), "got: {}", result);
    }

    // ─── Table rendering tests ──────────────────────────────────────

    #[test]
    fn test_render_table_basic() {
        let output = make_output(
            OutputType::Table,
            None,
            false,
            vec![("name", "string"), ("size", "filesize"), ("kind", "string")],
        );
        let records = vec![
            serde_json::json!({"name": "foo.txt", "size": 1024, "kind": "file"}),
            serde_json::json!({"name": "bar", "size": 4096, "kind": "dir"}),
        ];
        let mut buf = Vec::new();
        render_table(&records, &output, &mut buf);
        let result = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 rows
        assert!(lines[0].contains("KIND"));
        assert!(lines[0].contains("NAME"));
        assert!(lines[0].contains("SIZE"));
        assert!(lines[1].contains("foo.txt"));
        assert!(lines[1].contains("1 KiB"));
        assert!(lines[2].contains("bar"));
    }

    #[test]
    fn test_render_table_empty() {
        let output = make_output(OutputType::Table, None, false, vec![("name", "string")]);
        let mut buf = Vec::new();
        render_table(&[], &output, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "");
    }

    // ─── render_jsonl integration tests ─────────────────────────────

    #[test]
    fn test_render_jsonl_text() {
        let output = make_output(
            OutputType::Text,
            Some("text"),
            false,
            vec![("text", "string")],
        );
        let input = b"{\"text\":\"hello world\"}\n";
        let mut buf = Vec::new();
        render_jsonl(&input[..], &output, RenderMode::Rich, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "hello world\n");
    }

    #[test]
    fn test_render_jsonl_table() {
        let output = make_output(
            OutputType::Table,
            None,
            false,
            vec![("name", "string"), ("kind", "string")],
        );
        let input = b"{\"name\":\"a\",\"kind\":\"file\"}\n{\"name\":\"b\",\"kind\":\"dir\"}\n";
        let mut buf = Vec::new();
        render_jsonl(&input[..], &output, RenderMode::Rich, &mut buf);
        let result = String::from_utf8(buf).unwrap();
        assert!(result.contains("NAME"));
        assert!(result.contains("KIND"));
        assert!(result.contains("file"));
        assert!(result.contains("dir"));
    }

    #[test]
    fn test_render_jsonl_streaming_table() {
        let output = make_output(
            OutputType::Table,
            None,
            true,
            vec![("line", "number"), ("content", "string")],
        );
        let input = b"{\"line\":1,\"content\":\"a\"}\n{\"line\":2,\"content\":\"b\"}\n";
        let mut buf = Vec::new();
        render_jsonl(&input[..], &output, RenderMode::Rich, &mut buf);
        let result = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_render_jsonl_record() {
        let output = make_output(
            OutputType::Record,
            None,
            false,
            vec![("file", "string"), ("lines", "number")],
        );
        let input = b"{\"file\":\"test.txt\",\"lines\":42}\n";
        let mut buf = Vec::new();
        render_jsonl(&input[..], &output, RenderMode::Rich, &mut buf);
        let result = String::from_utf8(buf).unwrap();
        assert!(result.contains("file"));
        assert!(result.contains("test.txt"));
        assert!(result.contains("42"));
    }

    #[test]
    fn test_render_jsonl_plain_record() {
        let output = make_output(
            OutputType::Record,
            None,
            false,
            vec![("file", "string"), ("lines", "number")],
        );
        let input = b"{\"file\":\"test.txt\",\"lines\":42}\n";
        let mut buf = Vec::new();
        render_jsonl(&input[..], &output, RenderMode::Plain, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "test.txt\t42\n");
    }

    #[test]
    fn test_render_jsonl_plain_table() {
        let output = make_output(
            OutputType::Table,
            None,
            false,
            vec![("name", "string"), ("kind", "string")],
        );
        let input = b"{\"name\":\"a\",\"kind\":\"file\"}\n{\"name\":\"b\",\"kind\":\"dir\"}\n";
        let mut buf = Vec::new();
        render_jsonl(&input[..], &output, RenderMode::Plain, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "file\ta\ndir\tb\n");
    }

    #[test]
    fn test_render_streaming_plain_table_has_no_header() {
        let output = make_output(
            OutputType::Table,
            None,
            true,
            vec![("name", "string"), ("kind", "string")],
        );
        let record = serde_json::json!({"name": "a", "kind": "file"});
        let mut buf = Vec::new();
        render_streaming_header(&output, RenderMode::Plain, &mut buf);
        render_streaming_row(&record, &output, RenderMode::Plain, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "file\ta\n");
    }

    #[test]
    fn test_nested_json_in_table_cell() {
        let output = make_output(
            OutputType::Table,
            None,
            false,
            vec![("name", "string"), ("meta", "string")],
        );
        let records = vec![serde_json::json!({"name": "foo", "meta": {"author": "bar"}})];
        let mut buf = Vec::new();
        render_table(&records, &output, &mut buf);
        let result = String::from_utf8(buf).unwrap();
        assert!(result.contains(r#"{"author":"bar"}"#), "got: {}", result);
    }

    #[test]
    fn test_render_table_cjk_alignment() {
        let output = make_output(
            OutputType::Table,
            None,
            false,
            vec![("name", "string"), ("kind", "string")],
        );
        // CJK chars are 2 display columns each; "名前" = 4 display columns
        let records = vec![
            serde_json::json!({"name": "名前.txt", "kind": "file"}),
            serde_json::json!({"name": "hello.txt", "kind": "dir"}),
        ];
        let mut buf = Vec::new();
        render_table(&records, &output, &mut buf);
        let result = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        // "hello.txt" (9 display cols) is wider than "名前.txt" (8 display cols)
        // so column width should be 9. Both rows' KIND column should start at same position.
        let kind_pos_1 = lines[1].find("file").unwrap();
        let kind_pos_2 = lines[2].find("dir").unwrap();
        assert_eq!(
            kind_pos_1, kind_pos_2,
            "CJK alignment mismatch:\n{}",
            result
        );
    }
}
