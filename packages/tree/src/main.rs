zacor_package::single_command!("tree", |ctx| {
    let args = ctx.args::<zr_tree::args::DefaultArgs>()?;
    let depth = args.depth.map(|d| d as usize);
    let records = zr_tree::tree(&args.path, depth)?;
    for record in records {
        ctx.emit_record(&record)?;
    }
    Ok(0)
});
