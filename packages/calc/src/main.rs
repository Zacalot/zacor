zacor_package::single_command!("calc", |ctx| {
    let args = ctx.args::<zr_calc::args::DefaultArgs>()?;
    let record = zr_calc::calc(args)?;
    ctx.emit_record(&record)?;
    Ok(0)
});
