mod api;
mod init;

use crate::app::SharedAppState;
use anyhow::{Result, anyhow};
use mlua::Lua;

pub struct LuaRuntime {
    lua: Lua,
}

impl LuaRuntime {
    pub fn new(state: SharedAppState) -> Result<Self> {
        let lua = Lua::new();
        api::register(&lua, state).map_err(|error| anyhow!(error.to_string()))?;
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
