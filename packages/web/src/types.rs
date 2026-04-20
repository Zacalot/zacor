use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Web,
    News,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub query: String,
    pub engine: String,
    pub fallbacks: Vec<String>,
    pub count: usize,
    pub page: usize,
    pub timeout_secs: u64,
    pub mode: SearchMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchResult {
    pub engine: String,
    pub rank: i64,
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineFailure {
    pub engine: String,
    pub message: String,
}

impl EngineFailure {
    pub fn new(engine: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            engine: engine.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    pub url: String,
    pub timeout_secs: u64,
    pub user_agent: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FetchRecord {
    pub url: String,
    pub status: u16,
    pub elapsed: f64,
    pub body: String,
    pub content_type: String,
}
