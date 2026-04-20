zacor_package::single_command!("head", |ctx| {
    let args = ctx.args::<zr_head::args::DefaultArgs>()?;
    let input = ctx.input_or_empty();
    for record in zr_head::head(args.file, args.lines as usize, input)? {
        ctx.emit_record(&record)?;
    }
    Ok(0)
});
