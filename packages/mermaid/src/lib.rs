use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MermaidRender {
    pub svg: String,
}

pub fn render(source: &str) -> Result<MermaidRender, String> {
    let svg = mermaid_rs_renderer::render(source).map_err(|e| e.to_string())?;
    Ok(MermaidRender { svg })
}
