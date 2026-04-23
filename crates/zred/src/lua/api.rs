use crate::app::SharedAppState;
use mlua::{Lua, Result, Table};

pub fn register(lua: &Lua, state: SharedAppState) -> Result<()> {
    let globals = lua.globals();
    globals.set("app", app_api(lua, state.clone())?)?;
    globals.set("buffer", buffer_api(lua, state.clone())?)?;
    globals.set("minibuffer", minibuffer_api(lua, state)?)?;
    Ok(())
}

fn app_api(lua: &Lua, state: SharedAppState) -> Result<Table> {
    let app = lua.create_table()?;
    app.set(
        "quit",
        lua.create_function(move |_, ()| {
            state.borrow_mut().should_quit = true;
            Ok(())
        })?,
    )?;
    Ok(app)
}

fn buffer_api(lua: &Lua, state: SharedAppState) -> Result<Table> {
    let buffer = lua.create_table()?;

    let create_state = state.clone();
    buffer.set(
        "create",
        lua.create_function(move |_, name: String| {
            let mut state = create_state.borrow_mut();
            Ok(state.create_buffer(&name))
        })?,
    )?;

    let append_state = state.clone();
    buffer.set(
        "append",
        lua.create_function(move |_, (id, text): (u64, String)| {
            let mut state = append_state.borrow_mut();
            if !state.append_to_buffer(id, &text) {
                return Err(mlua::Error::external(format!("unknown buffer id: {id}")));
            }
            Ok(())
        })?,
    )?;

    let set_contents_state = state.clone();
    buffer.set(
        "set_contents",
        lua.create_function(move |_, (id, text): (u64, String)| {
            let mut state = set_contents_state.borrow_mut();
            if !state.set_buffer_contents(id, &text) {
                return Err(mlua::Error::external(format!("unknown buffer id: {id}")));
            }
            Ok(())
        })?,
    )?;

    let focus_state = state.clone();
    buffer.set(
        "focus",
        lua.create_function(move |_, id: u64| {
            let mut state = focus_state.borrow_mut();
            if !state.focus_buffer(id) {
                return Err(mlua::Error::external(format!("unknown buffer id: {id}")));
            }
            Ok(())
        })?,
    )?;

    Ok(buffer)
}

fn minibuffer_api(lua: &Lua, state: SharedAppState) -> Result<Table> {
    let minibuffer = lua.create_table()?;
    minibuffer.set(
        "message",
        lua.create_function(move |_, text: String| {
            state.borrow_mut().set_status(text);
            Ok(())
        })?,
    )?;
    Ok(minibuffer)
}
