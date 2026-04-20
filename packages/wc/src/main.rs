zacor_package::single_command!("wc", |ctx| {
    let args = ctx.args::<zr_wc::args::DefaultArgs>()?;
    let input = ctx.input_or_empty();
    let record = zr_wc::wc(args.file, input)?;
    ctx.emit_record(&record)?;
    Ok(0)
});
