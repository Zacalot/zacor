use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Line {
    pub text: String,
    pub record: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct Buffer {
    pub id: u64,
    pub name: String,
    pub lines: Vec<Line>,
}
