zacor_package::include_manifest!();

fn main() {
    std::process::exit(zacor_package::protocol(
        "zr-treesitter",
        |ctx| -> Result<i32, String> {
            match ctx.command() {
                "parse" => {
                    let args = ctx.raw_args();
                    let source = args.get("source").and_then(|v| v.as_str()).unwrap_or("");
                    let ext = args.get("ext").and_then(|v| v.as_str()).unwrap_or("");
                    let rel_path = args.get("rel_path").and_then(|v| v.as_str()).unwrap_or("");
                    let records = zr_treesitter::parse(source, ext, rel_path)
                        .into_iter()
                        .map(|decl| serde_json::to_value(decl).map_err(|e| e.to_string()))
                        .collect::<Result<Vec<_>, _>>()?;
                    ctx.emit_all(records)?;
                    Ok(0)
                }
                other => Err(format!("unknown command: {other}")),
            }
        },
    ));
}
