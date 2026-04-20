zacor_package::single_command!("watch", |ctx| {
    let args = ctx.args::<zr_watch::args::DefaultArgs>()?;
    for record in zr_watch::watch(args.path, args.no_recursive)? {
        ctx.emit_record(&record)?;
    }
    Ok(0)
});
