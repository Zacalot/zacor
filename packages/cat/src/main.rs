zacor_package::single_command!("cat", |ctx| {
    let args = ctx.args::<zr_cat::args::DefaultArgs>()?;
    let input = ctx.input_or_empty();
    for record in zr_cat::cat(args.file, args.lines, args.tail, input)? {
        ctx.emit_record(&record)?;
    }
    Ok(0)
});
