zacor_package::single_command!("where", |ctx| {
    let args = ctx.args::<zr_where::args::DefaultArgs>()?;
    let input = ctx.input_or_empty();
    let records = zr_where::filter(&args.expr, input)?;
    ctx.emit_all(records)?;
    Ok(0)
});
