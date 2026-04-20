zacor_package::single_command!("ls", |ctx| {
    let args = ctx.args::<zr_ls::args::DefaultArgs>()?;
    let records = zr_ls::ls(args.path, args.all)?;
    for record in records {
        ctx.emit_record(&record)?;
    }
    Ok(0)
});
