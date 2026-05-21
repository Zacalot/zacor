use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use mlua::{Function, Lua, RegistryKey, Table, Value};

use crate::function::{
    FunctionContext, FunctionError, FunctionInvoker, FunctionMetadata, FunctionName,
    FunctionOutcome, FunctionRegistry, LuaFunctionId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LuaHostPhase {
    Init,
    Load,
    Register,
    Call,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaHostError {
    phase: LuaHostPhase,
    source: Option<String>,
    message: String,
}

impl LuaHostError {
    fn new(phase: LuaHostPhase, source: Option<String>, message: impl Into<String>) -> Self {
        Self {
            phase,
            source,
            message: message.into(),
        }
    }

    pub fn phase(&self) -> LuaHostPhase {
        self.phase
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub struct LuaHost {
    lua: Lua,
    functions: Rc<RefCell<FunctionRegistry>>,
    lua_functions: Rc<RefCell<BTreeMap<LuaFunctionId, RegistryKey>>>,
    next_lua_function_id: Rc<Cell<u64>>,
}

impl LuaHost {
    pub fn new() -> Result<Self, LuaHostError> {
        let lua = Lua::new();
        let functions = Rc::new(RefCell::new(FunctionRegistry::new()));
        let lua_functions = Rc::new(RefCell::new(BTreeMap::new()));
        let next_lua_function_id = Rc::new(Cell::new(1));

        install_zri_api(
            &lua,
            functions.clone(),
            lua_functions.clone(),
            next_lua_function_id.clone(),
        )
        .map_err(|error| LuaHostError::new(LuaHostPhase::Init, None, error.to_string()))?;

        Ok(Self {
            lua,
            functions,
            lua_functions,
            next_lua_function_id,
        })
    }

    pub fn load_chunk(
        &self,
        source_name: impl Into<String>,
        source: &str,
    ) -> Result<(), LuaHostError> {
        let source_name = source_name.into();
        self.lua
            .load(source)
            .set_name(&source_name)
            .exec()
            .map_err(|error| {
                LuaHostError::new(LuaHostPhase::Load, Some(source_name), error.to_string())
            })
    }

    pub fn functions(&self) -> Vec<FunctionMetadata> {
        self.functions.borrow().entries().cloned().collect()
    }

    pub fn contains_function(&self, name: &str) -> bool {
        self.functions.borrow().contains(name)
    }

    pub fn call_function(&self, name: &str) -> Result<FunctionOutcome, LuaHostError> {
        self.call_function_by_name(name)
    }

    fn call_function_by_name(&self, name: &str) -> Result<FunctionOutcome, LuaHostError> {
        let id = self
            .functions
            .borrow()
            .lua_function_id(name)
            .ok_or_else(|| {
                LuaHostError::new(
                    LuaHostPhase::Call,
                    Some(name.to_string()),
                    format!("unknown function: {name}"),
                )
            })?;

        let function = {
            let lua_functions = self.lua_functions.borrow();
            let key = lua_functions.get(&id).ok_or_else(|| {
                LuaHostError::new(
                    LuaHostPhase::Call,
                    Some(name.to_string()),
                    format!("missing Lua function ref for: {name}"),
                )
            })?;
            self.lua.registry_value::<Function>(key).map_err(|error| {
                LuaHostError::new(
                    LuaHostPhase::Call,
                    Some(name.to_string()),
                    error.to_string(),
                )
            })?
        };

        let context_logs = Rc::new(RefCell::new(Vec::new()));
        let context =
            create_function_context(&self.lua, context_logs.clone()).map_err(|error| {
                LuaHostError::new(
                    LuaHostPhase::Call,
                    Some(name.to_string()),
                    error.to_string(),
                )
            })?;

        function.call::<Value>(context).map_err(|error| {
            LuaHostError::new(
                LuaHostPhase::Call,
                Some(name.to_string()),
                error.to_string(),
            )
        })?;

        let mut function_context = FunctionContext::new();
        for log in context_logs.borrow_mut().drain(..) {
            function_context.log(log);
        }
        Ok(function_context.finish())
    }

    pub fn next_lua_function_id(&self) -> u64 {
        self.next_lua_function_id.get()
    }
}

impl FunctionInvoker for LuaHost {
    type Error = LuaHostError;

    fn invoke_function(&self, name: &FunctionName) -> Result<FunctionOutcome, Self::Error> {
        self.call_function_by_name(name.as_str())
    }
}

fn install_zri_api(
    lua: &Lua,
    functions: Rc<RefCell<FunctionRegistry>>,
    lua_functions: Rc<RefCell<BTreeMap<LuaFunctionId, RegistryKey>>>,
    next_lua_function_id: Rc<Cell<u64>>,
) -> mlua::Result<()> {
    let zri = lua.create_table()?;
    let register_function =
        lua.create_function(move |lua, (name, function): (String, Function)| {
            let name = FunctionName::new(name).map_err(function_name_error)?;
            let registry_key = lua.create_registry_value(function)?;
            let function_id = LuaFunctionId(next_lua_function_id.get());
            next_lua_function_id.set(function_id.0 + 1);

            let previous = functions.borrow_mut().register_lua(name, function_id);
            if let Some(previous_id) = previous {
                lua_functions.borrow_mut().remove(&previous_id);
            }
            lua_functions.borrow_mut().insert(function_id, registry_key);
            Ok(())
        })?;
    zri.set("register_function", register_function)?;
    lua.globals().set("zri", zri)?;
    Ok(())
}

fn create_function_context(lua: &Lua, logs: Rc<RefCell<Vec<String>>>) -> mlua::Result<Table> {
    let context = lua.create_table()?;
    let log = lua.create_function(move |_, message: String| {
        logs.borrow_mut().push(message);
        Ok(())
    })?;
    context.set("log", log)?;
    Ok(context)
}

fn function_name_error(error: FunctionError) -> mlua::Error {
    mlua::Error::external(error.message().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoke_through_trait<I: FunctionInvoker>(
        invoker: &I,
        name: &FunctionName,
    ) -> Result<FunctionOutcome, I::Error> {
        invoker.invoke_function(name)
    }

    #[test]
    fn lua_host_starts_with_empty_registry() {
        let host = LuaHost::new().unwrap();

        assert!(host.functions().is_empty());
        assert_eq!(host.next_lua_function_id(), 1);
    }

    #[test]
    fn load_chunk_registers_function() {
        let host = LuaHost::new().unwrap();

        host.load_chunk(
            "test.lua",
            r#"
                zri.register_function("demo.hello", function(ctx)
                    ctx.log("hello")
                end)
            "#,
        )
        .unwrap();

        assert!(host.contains_function("demo.hello"));
        assert_eq!(host.functions().len(), 1);
        assert_eq!(host.functions()[0].name.as_str(), "demo.hello");
    }

    #[test]
    fn call_registered_function_returns_logs() {
        let host = LuaHost::new().unwrap();
        host.load_chunk(
            "test.lua",
            r#"
                zri.register_function("demo.hello", function(ctx)
                    ctx.log("hello from lua")
                end)
            "#,
        )
        .unwrap();

        let outcome = host.call_function("demo.hello").unwrap();

        assert_eq!(outcome.logs(), &["hello from lua".to_string()]);
    }

    #[test]
    fn lua_host_invokes_functions_through_generic_trait() {
        let host = LuaHost::new().unwrap();
        host.load_chunk(
            "test.lua",
            r#"
                zri.register_function("demo.hello", function(ctx)
                    ctx.log("hello from invoker")
                end)
            "#,
        )
        .unwrap();

        let outcome =
            invoke_through_trait(&host, &FunctionName::new("demo.hello").unwrap()).unwrap();

        assert_eq!(outcome.logs(), &["hello from invoker".to_string()]);
    }

    #[test]
    fn register_function_replaces_previous_function() {
        let host = LuaHost::new().unwrap();
        host.load_chunk(
            "first.lua",
            r#"
                zri.register_function("demo.hello", function(ctx)
                    ctx.log("first")
                end)
            "#,
        )
        .unwrap();
        host.load_chunk(
            "second.lua",
            r#"
                zri.register_function("demo.hello", function(ctx)
                    ctx.log("second")
                end)
            "#,
        )
        .unwrap();

        let outcome = host.call_function("demo.hello").unwrap();

        assert_eq!(outcome.logs(), &["second".to_string()]);
        assert_eq!(host.functions().len(), 1);
    }

    #[test]
    fn load_chunk_reports_syntax_error() {
        let host = LuaHost::new().unwrap();

        let error = host.load_chunk("broken.lua", "function(").unwrap_err();

        assert_eq!(error.phase(), LuaHostPhase::Load);
        assert_eq!(error.source(), Some("broken.lua"));
        assert!(!error.message().is_empty());
    }

    #[test]
    fn call_function_reports_runtime_error() {
        let host = LuaHost::new().unwrap();
        host.load_chunk(
            "error.lua",
            r#"
                zri.register_function("demo.fail", function(_ctx)
                    error("boom")
                end)
            "#,
        )
        .unwrap();

        let error = host.call_function("demo.fail").unwrap_err();

        assert_eq!(error.phase(), LuaHostPhase::Call);
        assert_eq!(error.source(), Some("demo.fail"));
        assert!(error.message().contains("boom"));
    }

    #[test]
    fn call_missing_function_reports_error() {
        let host = LuaHost::new().unwrap();

        let error = host.call_function("missing").unwrap_err();

        assert_eq!(error.phase(), LuaHostPhase::Call);
        assert_eq!(error.source(), Some("missing"));
        assert_eq!(error.message(), "unknown function: missing");
    }

    #[test]
    fn register_function_rejects_empty_name() {
        let host = LuaHost::new().unwrap();

        let error = host
            .load_chunk(
                "empty.lua",
                r#"
                    zri.register_function("   ", function(_ctx)
                    end)
                "#,
            )
            .unwrap_err();

        assert_eq!(error.phase(), LuaHostPhase::Load);
        assert_eq!(error.source(), Some("empty.lua"));
        assert!(error.message().contains("function name must not be empty"));
    }

    #[test]
    fn register_function_requires_function_value() {
        let host = LuaHost::new().unwrap();

        let error = host
            .load_chunk("bad.lua", "zri.register_function('demo.bad', 'nope')")
            .unwrap_err();

        assert_eq!(error.phase(), LuaHostPhase::Load);
        assert_eq!(error.source(), Some("bad.lua"));
        assert!(!error.message().is_empty());
    }
}
