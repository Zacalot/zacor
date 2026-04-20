zacor_package::include_manifest!();

fn main() {
    std::process::exit(zacor_package::protocol(
        "web",
        |ctx| -> Result<i32, String> {
            match ctx.command() {
                "search" => {
                    let args = ctx.args::<zr_web::args::SearchArgs>()?;
                    let records = zr_web::run_search(args)?;
                    for record in records {
                        ctx.emit_record(&record)?;
                    }
                    Ok(0)
                }
                "fetch" => {
                    let args = ctx.args::<zr_web::args::FetchArgs>()?;
                    let record = zr_web::run_fetch(args)?;
                    ctx.emit_record(&record)?;
                    Ok(0)
                }
                "extract" => {
                    let args = ctx.args::<zr_web::args::ExtractArgs>()?;
                    zr_web::run_extract(args)?;
                    Ok(0)
                }
                other => Err(format!("web: unknown command '{other}'")),
            }
        },
    ));
}
