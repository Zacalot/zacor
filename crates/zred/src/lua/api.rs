use crate::kernel::BufferId;
use crate::session::{LuaBufferApi, LuaCommandApi, LuaJobApi, LuaMinibufferApi};
use mlua::{Lua, Result, Table};

pub fn register(
    lua: &Lua,
    command_api_handle: LuaCommandApi,
    buffer_api_handle: LuaBufferApi,
    job_api_handle: LuaJobApi,
    minibuffer_api_handle: LuaMinibufferApi,
) -> Result<()> {
    let globals = lua.globals();
    globals.set("app", app_api(lua, command_api_handle.clone())?)?;
    globals.set("command", command_api(lua, command_api_handle)?)?;
    globals.set("buffer", buffer_api(lua, buffer_api_handle)?)?;
    globals.set("job", job_api(lua, job_api_handle)?)?;
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
    let new_window_api = command_api_handle.clone();
    app.set(
        "new_window",
        lua.create_function(move |_, ()| {
            new_window_api
                .new_window()
                .map_err(mlua::Error::external)?;
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

    let open_browser_api = buffer_api_handle.clone();
    buffer.set(
        "open_browser",
        lua.create_function(move |_, url: String| {
            let buffer_id = open_browser_api
                .open_browser(&url)
                .map_err(mlua::Error::external)?;
            Ok(buffer_id.raw())
        })?,
    )?;

    let open_media_api = buffer_api_handle.clone();
    buffer.set(
        "open_media",
        lua.create_function(move |_, source: String| {
            let buffer_id = open_media_api
                .open_media(&source)
                .map_err(mlua::Error::external)?;
            Ok(buffer_id.raw())
        })?,
    )?;

    let open_canvas_api = buffer_api_handle.clone();
    buffer.set(
        "open_canvas",
        lua.create_function(move |_, name: String| {
            let buffer_id = open_canvas_api
                .open_canvas(&name)
                .map_err(mlua::Error::external)?;
            Ok(buffer_id.raw())
        })?,
    )?;

    let open_terminal_api = buffer_api_handle.clone();
    buffer.set(
        "open_terminal",
        lua.create_function(move |_, name: String| {
            let buffer_id = open_terminal_api
                .open_terminal(&name)
                .map_err(mlua::Error::external)?;
            Ok(buffer_id.raw())
        })?,
    )?;

    let append_terminal_api = buffer_api_handle.clone();
    buffer.set(
        "append_terminal",
        lua.create_function(move |_, (id, text): (u64, String)| {
            append_terminal_api
                .append_terminal(BufferId::new(id), &text)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let append_current_terminal_api = buffer_api_handle.clone();
    buffer.set(
        "append_current_terminal",
        lua.create_function(move |_, text: String| {
            append_current_terminal_api
                .append_current_terminal(&text)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let current_buffer_api = buffer_api_handle.clone();
    buffer.set(
        "current",
        lua.create_function(move |lua, ()| {
            let info = current_buffer_api.current_buffer();
            let item = lua.create_table()?;
            item.set("id", info.id)?;
            item.set("name", info.name)?;
            item.set("kind", info.kind)?;
            item.set("text_line_count", info.text_line_count)?;
            item.set("record_count", info.record_count)?;
            item.set("browser_url", info.browser_url)?;
            item.set("browser_title", info.browser_title)?;
            item.set("media_source", info.media_source)?;
            item.set("canvas_name", info.canvas_name)?;
            Ok(item)
        })?,
    )?;

    let select_record_api = buffer_api_handle.clone();
    buffer.set(
        "select_record",
        lua.create_function(move |_, row: usize| {
            select_record_api
                .select_record_row(row)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let current_record_api = buffer_api_handle.clone();
    buffer.set(
        "current_record",
        lua.create_function(move |lua, ()| match current_record_api.current_record() {
            Some(record) => {
                let item = lua.create_table()?;
                item.set("row", record.row + 1)?;
                item.set("value", mlua::LuaSerdeExt::to_value(lua, &record.value)?)?;
                Ok(Some(item))
            }
            None => Ok(None),
        })?,
    )?;

    let current_structured_api = buffer_api_handle.clone();
    buffer.set(
        "current_structured_item",
        lua.create_function(move |lua, ()| {
            match current_structured_api.current_structured_item() {
                Some(item) => Ok(Some(mlua::LuaSerdeExt::to_value(lua, &item)?)),
                None => Ok(None),
            }
        })?,
    )?;

    let next_record_api = buffer_api_handle.clone();
    buffer.set(
        "next_record",
        lua.create_function(move |_, ()| {
            next_record_api
                .next_record()
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let next_structured_api = buffer_api_handle.clone();
    buffer.set(
        "next_structured_item",
        lua.create_function(move |_, ()| {
            next_structured_api
                .next_structured_item()
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let open_record_api = buffer_api_handle.clone();
    buffer.set(
        "open_record",
        lua.create_function(move |_, ()| {
            open_record_api
                .open_record()
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let open_structured_api = buffer_api_handle.clone();
    buffer.set(
        "open_structured_item",
        lua.create_function(move |_, ()| {
            open_structured_api
                .open_structured_item()
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let previous_record_api = buffer_api_handle.clone();
    buffer.set(
        "prev_record",
        lua.create_function(move |_, ()| {
            previous_record_api
                .previous_record()
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let previous_structured_api = buffer_api_handle.clone();
    buffer.set(
        "prev_structured_item",
        lua.create_function(move |_, ()| {
            previous_structured_api
                .previous_structured_item()
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let current_tree_api = buffer_api_handle.clone();
    buffer.set(
        "current_tree_node",
        lua.create_function(move |lua, ()| match current_tree_api.current_tree_node() {
            Some(node) => {
                let item = lua.create_table()?;
                item.set("id", node.id)?;
                item.set("label", node.label)?;
                item.set("linked_buffer_id", node.linked_buffer_id)?;
                Ok(Some(item))
            }
            None => Ok(None),
        })?,
    )?;

    let open_tree_api = buffer_api_handle.clone();
    buffer.set(
        "open_tree_node",
        lua.create_function(move |_, ()| {
            open_tree_api
                .open_tree_node()
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let next_tree_api = buffer_api_handle.clone();
    buffer.set(
        "next_tree_node",
        lua.create_function(move |_, ()| {
            next_tree_api
                .next_tree_node()
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let previous_tree_api = buffer_api_handle.clone();
    buffer.set(
        "prev_tree_node",
        lua.create_function(move |_, ()| {
            previous_tree_api
                .previous_tree_node()
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let save_workspace_api = buffer_api_handle.clone();
    buffer.set(
        "save_workspace",
        lua.create_function(move |_, path: String| {
            save_workspace_api
                .save_workspace(&path)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let load_workspace_api = buffer_api_handle.clone();
    buffer.set(
        "load_workspace",
        lua.create_function(move |_, path: String| {
            load_workspace_api
                .load_workspace(&path)
                .map_err(mlua::Error::external)?;
            Ok(())
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

    let set_browser_title_api = buffer_api_handle.clone();
    buffer.set(
        "set_browser_title",
        lua.create_function(move |_, (id, title): (u64, String)| {
            set_browser_title_api
                .set_browser_title(BufferId::new(id), &title)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let set_current_browser_title_api = buffer_api_handle.clone();
    buffer.set(
        "set_current_browser_title",
        lua.create_function(move |_, title: String| {
            set_current_browser_title_api
                .set_current_browser_title(&title)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let set_browser_url_api = buffer_api_handle.clone();
    buffer.set(
        "set_browser_url",
        lua.create_function(move |_, (id, url): (u64, String)| {
            set_browser_url_api
                .set_browser_url(BufferId::new(id), &url)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let set_current_browser_url_api = buffer_api_handle.clone();
    buffer.set(
        "set_current_browser_url",
        lua.create_function(move |_, url: String| {
            set_current_browser_url_api
                .set_current_browser_url(&url)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let set_media_source_api = buffer_api_handle.clone();
    buffer.set(
        "set_media_source",
        lua.create_function(move |_, (id, source): (u64, String)| {
            set_media_source_api
                .set_media_source(BufferId::new(id), &source)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let set_current_media_source_api = buffer_api_handle.clone();
    buffer.set(
        "set_current_media_source",
        lua.create_function(move |_, source: String| {
            set_current_media_source_api
                .set_current_media_source(&source)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let set_canvas_name_api = buffer_api_handle.clone();
    buffer.set(
        "set_canvas_name",
        lua.create_function(move |_, (id, name): (u64, String)| {
            set_canvas_name_api
                .set_canvas_name(BufferId::new(id), &name)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    let set_current_canvas_name_api = buffer_api_handle.clone();
    buffer.set(
        "set_current_canvas_name",
        lua.create_function(move |_, name: String| {
            set_current_canvas_name_api
                .set_current_canvas_name(&name)
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

fn job_api(lua: &Lua, job_api_handle: LuaJobApi) -> Result<Table> {
    let job = lua.create_table()?;
    let get_api = job_api_handle.clone();
    job.set(
        "get",
        lua.create_function(move |lua, id: u64| {
            let Some(entry) = get_api.job(id) else {
                return Ok(mlua::Value::Nil);
            };

            let item = lua.create_table()?;
            item.set("id", entry.id)?;
            item.set("name", entry.name)?;
            item.set("status", entry.status)?;
            item.set("status_message", entry.status_message)?;
            item.set("owner_kind", entry.owner_kind)?;
            item.set("owner_id", entry.owner_id)?;
            item.set("kind", entry.kind)?;
            item.set("package", entry.package)?;
            item.set("command", entry.command)?;
            item.set("output_buffer_id", entry.output_buffer_id)?;
            Ok(mlua::Value::Table(item))
        })?,
    )?;
    let list_api = job_api_handle.clone();
    job.set(
        "list",
        lua.create_function(move |lua, ()| {
            let jobs = lua.create_table()?;
            for (index, entry) in list_api.jobs().into_iter().enumerate() {
                let item = lua.create_table()?;
                item.set("id", entry.id)?;
                item.set("name", entry.name)?;
                item.set("status", entry.status)?;
                item.set("status_message", entry.status_message)?;
                item.set("owner_kind", entry.owner_kind)?;
                item.set("owner_id", entry.owner_id)?;
                item.set("kind", entry.kind)?;
                item.set("package", entry.package)?;
                item.set("command", entry.command)?;
                item.set("output_buffer_id", entry.output_buffer_id)?;
                jobs.set(index + 1, item)?;
            }
            Ok(jobs)
        })?,
    )?;
    let current_api = job_api_handle.clone();
    job.set(
        "current",
        lua.create_function(move |lua, ()| {
            let Some(entry) = current_api.current_job() else {
                return Ok(mlua::Value::Nil);
            };

            let item = lua.create_table()?;
            item.set("id", entry.id)?;
            item.set("name", entry.name)?;
            item.set("status", entry.status)?;
            item.set("status_message", entry.status_message)?;
            item.set("owner_kind", entry.owner_kind)?;
            item.set("owner_id", entry.owner_id)?;
            item.set("kind", entry.kind)?;
            item.set("package", entry.package)?;
            item.set("command", entry.command)?;
            item.set("output_buffer_id", entry.output_buffer_id)?;
            Ok(mlua::Value::Table(item))
        })?,
    )?;
    let open_api = job_api_handle.clone();
    job.set(
        "open",
        lua.create_function(move |_, ()| {
            let buffer_id = open_api.open_jobs_buffer().map_err(mlua::Error::external)?;
            Ok(buffer_id.raw())
        })?,
    )?;
    let next_api = job_api_handle.clone();
    job.set(
        "next",
        lua.create_function(move |_, ()| {
            next_api.next().map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;
    let prev_api = job_api_handle.clone();
    job.set(
        "prev",
        lua.create_function(move |_, ()| {
            prev_api.previous().map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;
    let focus_output_api = job_api_handle.clone();
    job.set(
        "focus_output",
        lua.create_function(move |_, id: Option<u64>| {
            focus_output_api
                .focus_output(id)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;
    Ok(job)
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
