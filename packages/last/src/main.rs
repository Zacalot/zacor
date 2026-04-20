zacor_package::single_command!("last", |ctx| {
    let args = ctx.args::<zr_last::args::DefaultArgs>()?;
    let input = ctx.input_or_empty();
    let records = zr_last::last(args.count, input)?;
    ctx.emit_all(records)?;
    Ok(0)
});
