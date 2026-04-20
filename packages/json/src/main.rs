zacor_package::single_command!("json", |ctx| {
    let args = ctx.args::<zr_json::args::DefaultArgs>()?;
    let input = ctx.input_or_empty();
    let record = zr_json::json(args.indent as usize, args.compact, args.validate, input)?;
    ctx.emit_record(&record)?;
    Ok(0)
});
