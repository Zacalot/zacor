zacor_package::include_manifest!();

fn main() {
    std::process::exit(zacor_package::protocol(
        "mermaid",
        |ctx| -> Result<i32, String> {
            match ctx.command() {
                "render" => {
                    let source = ctx
                        .raw_args()
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let record = zr_mermaid::render(source)?;
                    ctx.emit_record(&record)?;
                    Ok(0)
                }
                other => Err(format!("unknown command: {other}")),
            }
        },
    ));
}
