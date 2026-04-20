zacor_package::single_command!("hash", |ctx| {
    let args = ctx.args::<zr_hash::args::DefaultArgs>()?;
    let input = ctx.input_or_empty();
    for record in zr_hash::hash(args.file, &args.algorithm, input)? {
        ctx.emit_record(&record)?;
    }
    Ok(0)
});
