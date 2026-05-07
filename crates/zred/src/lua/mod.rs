mod api;
mod init;

use crate::session::{LuaBufferApi, LuaCommandApi, LuaJobApi, LuaMinibufferApi, SharedSession};
use anyhow::{Result, anyhow};
use mlua::Lua;

pub struct LuaRuntime {
    lua: Lua,
}

impl LuaRuntime {
    pub fn new(state: SharedSession) -> Result<Self> {
        let lua = Lua::new();
        api::register(
            &lua,
            LuaCommandApi::new(state.clone()),
            LuaBufferApi::new(state.clone()),
            LuaJobApi::new(state.clone()),
            LuaMinibufferApi::new(state),
        )
        .map_err(|error| anyhow!(error.to_string()))?;
        init::load_user_init(&lua).map_err(|error| anyhow!(error.to_string()))?;
        Ok(Self { lua })
    }

    pub fn eval(&self, script: &str) -> Result<()> {
        self.lua
            .load(script)
            .exec()
            .map_err(|error| anyhow!(error.to_string()))?;
        Ok(())
    }
}
