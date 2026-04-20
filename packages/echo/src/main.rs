zacor_package::single_command!("echo", |ctx| {
    let args = ctx.args::<zr_echo::args::DefaultArgs>()?;
    let record = zr_echo::echo(args)?;
    ctx.emit_record(&record)?;
    Ok(0)
});
