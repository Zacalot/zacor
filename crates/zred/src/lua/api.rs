use crate::kernel::BufferId;
use crate::session::{LuaBufferApi, LuaCommandApi, LuaMinibufferApi};
use mlua::{Lua, Result, Table};

pub fn register(
    lua: &Lua,
    command_api_handle: LuaCommandApi,
    buffer_api_handle: LuaBufferApi,
    minibuffer_api_handle: LuaMinibufferApi,
) -> Result<()> {
    let globals = lua.globals();
    globals.set("app", app_api(lua, command_api_handle.clone())?)?;
    globals.set("command", command_api(lua, command_api_handle)?)?;
    globals.set("buffer", buffer_api(lua, buffer_api_handle)?)?;
    globals.set("minibuffer", minibuffer_api(lua, minibuffer_api_handle)?)?;
    Ok(())
}

fn app_api(lua: &Lua, command_api_handle: LuaCommandApi) -> Result<Table> {
    let app = lua.create_table()?;
    let quit_api = command_api_handle.clone();
    app.set(
        "quit",
        lua.create_function(move |_, ()| {
            quit_api.quit().map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;
    Ok(app)
}

fn buffer_api(lua: &Lua, buffer_api_handle: LuaBufferApi) -> Result<Table> {
    let buffer = lua.create_table()?;

    let create_api = buffer_api_handle.clone();
    buffer.set(
        "create",
        lua.create_function(move |_, name: String| {
            let buffer_id = create_api
                .create_buffer(&name)
                .map_err(mlua::Error::external)?;
            Ok(buffer_id.raw())
        })?,
    )?;

    let append_api = buffer_api_handle.clone();
    buffer.set(
        "append",
        lua.create_function(move |_, (id, text): (u64, String)| {
            append_api
                .append_to_buffer(BufferId::new(id), &text)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let set_contents_api = buffer_api_handle.clone();
    buffer.set(
        "set_contents",
        lua.create_function(move |_, (id, text): (u64, String)| {
            set_contents_api
                .set_buffer_contents(BufferId::new(id), &text)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let focus_api = buffer_api_handle.clone();
    buffer.set(
        "focus",
        lua.create_function(move |_, id: u64| {
            focus_api
                .focus_buffer(BufferId::new(id))
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    Ok(buffer)
}

fn command_api(lua: &Lua, command_api_handle: LuaCommandApi) -> Result<Table> {
    let command = lua.create_table()?;
    let get_api = command_api_handle.clone();
    command.set(
        "get",
        lua.create_function(move |lua, name: String| {
            let Some(entry) = get_api.command(&name) else {
                return Ok(mlua::Value::Nil);
            };

            let item = lua.create_table()?;
            item.set("name", entry.name)?;
            item.set("summary", entry.summary)?;
            item.set("scope", entry.scope)?;
            item.set("usage", entry.usage)?;
            Ok(mlua::Value::Table(item))
        })?,
    )?;
    let list_api = command_api_handle.clone();
    command.set(
        "list",
        lua.create_function(move |lua, ()| {
            let commands = lua.create_table()?;
            for (index, entry) in list_api.commands().into_iter().enumerate() {
                let item = lua.create_table()?;
                item.set("name", entry.name)?;
                item.set("summary", entry.summary)?;
                item.set("scope", entry.scope)?;
                item.set("usage", entry.usage)?;
                commands.set(index + 1, item)?;
            }
            Ok(commands)
        })?,
    )?;
    command.set(
        "run",
        lua.create_function(move |_, input: String| {
            command_api_handle
                .run_command(&input)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;
    Ok(command)
}

fn minibuffer_api(lua: &Lua, minibuffer_api_handle: LuaMinibufferApi) -> Result<Table> {
    let minibuffer = lua.create_table()?;
    minibuffer.set(
        "message",
        lua.create_function(move |_, text: String| {
            minibuffer_api_handle.set_message(text);
            Ok(())
        })?,
    )?;
    Ok(minibuffer)
}
