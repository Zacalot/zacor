use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::host::{Axis, BufferId};

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

/// A Rust-native function handler. Handlers never receive host state; they
/// describe their intent as effects through the context (the single
/// chokepoint where capability gating can later attach as policy).
pub type RustHandler = Arc<dyn Fn(&mut FunctionContext) + 'static>;

enum FunctionHandler {
    Rust(RustHandler),
    Lua(LuaFunctionId),
}

struct RegisteredFunction {
    metadata: FunctionMetadata,
    handler: FunctionHandler,
}

#[derive(Default)]
pub struct FunctionRegistry {
    entries: BTreeMap<FunctionName, RegisteredFunction>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_rust(&mut self, name: FunctionName, handler: RustHandler) -> bool {
        let previous = self.entries.insert(
            name.clone(),
            RegisteredFunction {
                metadata: FunctionMetadata {
                    name,
                    source: FunctionSource::Rust,
                },
                handler: FunctionHandler::Rust(handler),
            },
        );
        previous.is_some()
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
            FunctionHandler::Rust(_) => None,
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
            .and_then(|registered| match registered.handler {
                FunctionHandler::Lua(id) => Some(id),
                FunctionHandler::Rust(_) => None,
            })
    }

    pub fn rust_handler(&self, name: &str) -> Option<RustHandler> {
        self.entries
            .get(name)
            .and_then(|registered| match &registered.handler {
                FunctionHandler::Rust(handler) => Some(handler.clone()),
                FunctionHandler::Lua(_) => None,
            })
    }

    pub fn entries(&self) -> impl Iterator<Item = &FunctionMetadata> {
        self.entries.values().map(|registered| &registered.metadata)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A side effect described by a function and applied by the runtime after the
/// call returns (buffered-then-committed, never reentrant). Functions never
/// mutate host/render/input state directly — effects are the only door, which
/// keeps the surface handle-shaped (Neovim `nvim_buf_*` precedent) and gives
/// capability gating a single later attachment point (Zed granter precedent).
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FunctionEffect {
    BufferAppend {
        buffer: BufferId,
        text: String,
    },
    /// Split whichever pane is active when the effect applies (Emacs
    /// split-selected-window semantics; resolved by the runtime at apply
    /// time, like every other effect target).
    SplitActivePane {
        axis: Axis,
    },
    /// Move selection to the next focusable pane in tree order, wrapping.
    FocusNextPane,
    /// Create a fresh scratch text buffer and host it in the active pane.
    OpenScratchBuffer,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FunctionContext {
    logs: Vec<String>,
    effects: Vec<FunctionEffect>,
}

impl FunctionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log(&mut self, message: impl Into<String>) {
        self.logs.push(message.into());
    }

    /// Queue an append to a local buffer, applied by the runtime after the
    /// function returns. The target is revalidated at apply time.
    pub fn buf_append(&mut self, buffer: BufferId, text: impl Into<String>) {
        self.effects.push(FunctionEffect::BufferAppend {
            buffer,
            text: text.into(),
        });
    }

    /// Queue a split of the pane that is active when the effect applies.
    pub fn split_active_pane(&mut self, axis: Axis) {
        self.effects.push(FunctionEffect::SplitActivePane { axis });
    }

    /// Queue a selection move to the next focusable pane in tree order.
    pub fn focus_next_pane(&mut self) {
        self.effects.push(FunctionEffect::FocusNextPane);
    }

    /// Queue creation of a fresh scratch buffer hosted in the active pane.
    pub fn open_scratch_buffer(&mut self) {
        self.effects.push(FunctionEffect::OpenScratchBuffer);
    }

    pub fn push_effect(&mut self, effect: FunctionEffect) {
        self.effects.push(effect);
    }

    pub fn finish(self) -> FunctionOutcome {
        FunctionOutcome {
            logs: self.logs,
            effects: self.effects,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FunctionOutcome {
    logs: Vec<String>,
    effects: Vec<FunctionEffect>,
}

impl FunctionOutcome {
    pub fn logs(&self) -> &[String] {
        &self.logs
    }

    pub fn effects(&self) -> &[FunctionEffect] {
        &self.effects
    }
}

pub trait FunctionInvoker {
    type Error;

    fn invoke_function(&self, name: &FunctionName) -> Result<FunctionOutcome, Self::Error>;
}

/// One invocation's name and result, as reported in turn results.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionInvocation {
    pub function: FunctionName,
    pub result: Result<FunctionOutcome, FunctionError>,
}

/// Routes function invocations: Rust handlers first (its own registry), then
/// an optional delegate invoker (in practice the `LuaHost`). Name collisions
/// resolve Rust-first.
#[derive(Default)]
pub struct FunctionRouter {
    registry: FunctionRegistry,
    delegate: Option<Box<dyn Fn(&FunctionName) -> Result<FunctionOutcome, FunctionError>>>,
}

impl FunctionRouter {
    pub fn new(registry: FunctionRegistry) -> Self {
        Self {
            registry,
            delegate: None,
        }
    }

    pub fn with_delegate<I>(mut self, invoker: I) -> Self
    where
        I: FunctionInvoker + 'static,
        I::Error: std::fmt::Debug,
    {
        self.delegate = Some(Box::new(move |name| {
            invoker
                .invoke_function(name)
                .map_err(|error| FunctionError::new(format!("{error:?}")))
        }));
        self
    }

    pub fn registry(&self) -> &FunctionRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut FunctionRegistry {
        &mut self.registry
    }

    pub fn invoke(&self, name: &FunctionName) -> Result<FunctionOutcome, FunctionError> {
        if let Some(handler) = self.registry.rust_handler(name.as_str()) {
            let mut context = FunctionContext::new();
            handler(&mut context);
            return Ok(context.finish());
        }
        if let Some(delegate) = &self.delegate {
            return delegate(name);
        }
        Err(FunctionError::new(format!(
            "unknown function: {}",
            name.as_str()
        )))
    }
}

impl FunctionInvoker for FunctionRouter {
    type Error = FunctionError;

    fn invoke_function(&self, name: &FunctionName) -> Result<FunctionOutcome, Self::Error> {
        self.invoke(name)
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

    #[test]
    fn function_context_queues_effects() {
        let mut context = FunctionContext::new();
        context.buf_append(BufferId(3), "hello");

        let outcome = context.finish();

        assert_eq!(
            outcome.effects(),
            &[FunctionEffect::BufferAppend {
                buffer: BufferId(3),
                text: "hello".to_string(),
            }]
        );
    }

    #[test]
    fn registry_registers_rust_handler() {
        let mut registry = FunctionRegistry::new();
        let name = FunctionName::new("demo.rust").unwrap();

        let replaced = registry.register_rust(name.clone(), Arc::new(|_| {}));

        assert!(!replaced);
        assert!(registry.contains("demo.rust"));
        assert_eq!(
            registry.metadata("demo.rust").unwrap().source,
            FunctionSource::Rust
        );
        assert!(registry.rust_handler("demo.rust").is_some());
        assert_eq!(registry.lua_function_id("demo.rust"), None);
    }

    #[test]
    fn router_invokes_rust_handler_and_collects_effects() {
        let mut registry = FunctionRegistry::new();
        registry.register_rust(
            FunctionName::new("demo.stamp").unwrap(),
            Arc::new(|context| {
                context.log("stamped");
                context.buf_append(BufferId(1), "x");
            }),
        );
        let router = FunctionRouter::new(registry);

        let outcome = router
            .invoke(&FunctionName::new("demo.stamp").unwrap())
            .unwrap();

        assert_eq!(outcome.logs(), &["stamped".to_string()]);
        assert_eq!(outcome.effects().len(), 1);
    }

    #[test]
    fn router_reports_unknown_function() {
        let router = FunctionRouter::new(FunctionRegistry::new());

        let error = router
            .invoke(&FunctionName::new("missing").unwrap())
            .unwrap_err();

        assert_eq!(error.message(), "unknown function: missing");
    }

    #[test]
    fn router_falls_back_to_delegate_rust_first() {
        struct Delegate;
        impl FunctionInvoker for Delegate {
            type Error = FunctionError;
            fn invoke_function(
                &self,
                _name: &FunctionName,
            ) -> Result<FunctionOutcome, Self::Error> {
                let mut context = FunctionContext::new();
                context.log("from delegate");
                Ok(context.finish())
            }
        }

        let mut registry = FunctionRegistry::new();
        registry.register_rust(
            FunctionName::new("demo.shared").unwrap(),
            Arc::new(|context| context.log("from rust")),
        );
        let router = FunctionRouter::new(registry).with_delegate(Delegate);

        let rust = router
            .invoke(&FunctionName::new("demo.shared").unwrap())
            .unwrap();
        let delegated = router
            .invoke(&FunctionName::new("demo.lua_only").unwrap())
            .unwrap();

        assert_eq!(rust.logs(), &["from rust".to_string()]);
        assert_eq!(delegated.logs(), &["from delegate".to_string()]);
    }
}
