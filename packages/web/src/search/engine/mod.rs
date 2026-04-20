mod duckduckgo_html;
mod duckduckgo_lite;

use crate::types::{EngineFailure, SearchRequest, SearchResult};
use std::collections::BTreeMap;
use std::sync::Arc;

pub use duckduckgo_html::DuckDuckGoHtmlEngine;
pub use duckduckgo_lite::DuckDuckGoLiteEngine;

const SEARCH_CONTEXT: &str = "web search";

pub trait SearchEngine: Send + Sync {
    fn name(&self) -> &'static str;
    fn search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>, String>;
}

#[derive(Default)]
pub struct EngineRegistry {
    engines: BTreeMap<String, Arc<dyn SearchEngine>>,
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_engine<E: SearchEngine + 'static>(mut self, engine: E) -> Self {
        self.engines
            .insert(engine.name().to_string(), Arc::new(engine));
        self
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn SearchEngine>> {
        self.engines.get(name).cloned()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.engines.contains_key(name)
    }

    pub fn known_engines(&self) -> Vec<String> {
        self.engines.keys().cloned().collect()
    }

    pub fn default_fallbacks_for(&self, primary: &str) -> Vec<String> {
        self.known_engines()
            .into_iter()
            .filter(|name| name != primary)
            .collect()
    }
}

pub fn default_registry() -> EngineRegistry {
    EngineRegistry::new()
        .with_engine(DuckDuckGoHtmlEngine)
        .with_engine(DuckDuckGoLiteEngine)
}

pub fn execute_with_fallbacks(
    registry: &EngineRegistry,
    request: &SearchRequest,
) -> Result<Vec<SearchResult>, String> {
    let mut attempts = Vec::new();
    attempts.push(request.engine.clone());
    for fallback in &request.fallbacks {
        if !attempts.iter().any(|existing| existing == fallback) {
            attempts.push(fallback.clone());
        }
    }

    let mut failures = Vec::new();
    for engine_name in attempts {
        let Some(engine) = registry.get(&engine_name) else {
            failures.push(EngineFailure::new(engine_name, "engine is not registered"));
            continue;
        };

        match engine.search(request) {
            Ok(results) => return Ok(results),
            Err(message) => failures.push(EngineFailure::new(engine.name(), message)),
        }
    }

    let joined = failures
        .into_iter()
        .map(|failure| format!("{}: {}", failure.engine, failure.message))
        .collect::<Vec<_>>()
        .join("; ");
    Err(format!(
        "{SEARCH_CONTEXT}: no configured engine completed the request successfully ({joined})"
    ))
}
