use std::borrow::Borrow;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionError {
    message: String,
}

impl FunctionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FunctionName(String);

impl FunctionName {
    pub fn new(name: impl Into<String>) -> Result<Self, FunctionError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(FunctionError::new("function name must not be empty"));
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for FunctionName {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionSource {
    Rust,
    Lua,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LuaFunctionId(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionMetadata {
    pub name: FunctionName,
    pub source: FunctionSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FunctionHandler {
    Lua(LuaFunctionId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RegisteredFunction {
    metadata: FunctionMetadata,
    handler: FunctionHandler,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FunctionRegistry {
    entries: BTreeMap<FunctionName, RegisteredFunction>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_lua(&mut self, name: FunctionName, id: LuaFunctionId) -> Option<LuaFunctionId> {
        let previous = self.entries.insert(
            name.clone(),
            RegisteredFunction {
                metadata: FunctionMetadata {
                    name,
                    source: FunctionSource::Lua,
                },
                handler: FunctionHandler::Lua(id),
            },
        );

        previous.and_then(|registered| match registered.handler {
            FunctionHandler::Lua(id) => Some(id),
        })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    pub fn metadata(&self, name: &str) -> Option<&FunctionMetadata> {
        self.entries
            .get(name)
            .map(|registered| &registered.metadata)
    }

    pub fn lua_function_id(&self, name: &str) -> Option<LuaFunctionId> {
        self.entries
            .get(name)
            .map(|registered| match registered.handler {
                FunctionHandler::Lua(id) => id,
            })
    }

    pub fn entries(&self) -> impl Iterator<Item = &FunctionMetadata> {
        self.entries.values().map(|registered| &registered.metadata)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FunctionContext {
    logs: Vec<String>,
}

impl FunctionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log(&mut self, message: impl Into<String>) {
        self.logs.push(message.into());
    }

    pub fn finish(self) -> FunctionOutcome {
        FunctionOutcome { logs: self.logs }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FunctionOutcome {
    logs: Vec<String>,
}

impl FunctionOutcome {
    pub fn logs(&self) -> &[String] {
        &self.logs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_name_rejects_empty_names() {
        let error = FunctionName::new("   ").unwrap_err();
        assert_eq!(error.message(), "function name must not be empty");
    }

    #[test]
    fn function_registry_registers_lua_function() {
        let mut registry = FunctionRegistry::new();
        let name = FunctionName::new("demo.hello").unwrap();

        let previous = registry.register_lua(name.clone(), LuaFunctionId(1));

        assert_eq!(previous, None);
        assert!(registry.contains("demo.hello"));
        assert_eq!(
            registry.lua_function_id("demo.hello"),
            Some(LuaFunctionId(1))
        );
        assert_eq!(registry.metadata("demo.hello").unwrap().name, name);
        assert_eq!(
            registry.metadata("demo.hello").unwrap().source,
            FunctionSource::Lua
        );
    }

    #[test]
    fn function_registry_replaces_existing_function() {
        let mut registry = FunctionRegistry::new();
        let name = FunctionName::new("demo.hello").unwrap();
        let _ = registry.register_lua(name.clone(), LuaFunctionId(1));

        let previous = registry.register_lua(name, LuaFunctionId(2));

        assert_eq!(previous, Some(LuaFunctionId(1)));
        assert_eq!(
            registry.lua_function_id("demo.hello"),
            Some(LuaFunctionId(2))
        );
    }

    #[test]
    fn function_context_records_logs() {
        let mut context = FunctionContext::new();
        context.log("first");
        context.log("second");

        let outcome = context.finish();

        assert_eq!(outcome.logs(), &["first".to_string(), "second".to_string()]);
    }
}
