zacor_package::single_command!("first", |ctx| {
    let args = ctx.args::<zr_first::args::DefaultArgs>()?;
    let input = ctx.input_or_empty();
    let records = zr_first::first(args.count, input)?;
    ctx.emit_all(records)?;
    Ok(0)
});
