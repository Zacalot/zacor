use super::{Frame, Primitive};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderCapabilities {
    pub fills: bool,
    pub strokes: bool,
    pub lines: bool,
    pub text: bool,
}

impl RenderCapabilities {
    pub const fn all() -> Self {
        Self {
            fills: true,
            strokes: true,
            lines: true,
            text: true,
        }
    }

    pub const fn none() -> Self {
        Self {
            fills: false,
            strokes: false,
            lines: false,
            text: false,
        }
    }

    pub fn supports(&self, primitive: &Primitive) -> bool {
        match primitive {
            Primitive::Clear { .. } | Primitive::FillRect { .. } => self.fills,
            Primitive::StrokeRect { .. } => self.strokes,
            Primitive::Line { .. } => self.lines,
            Primitive::Text { .. } => self.text,
        }
    }
}

impl Default for RenderCapabilities {
    fn default() -> Self {
        Self::none()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderResult {
    pub rendered_primitives: usize,
    pub unsupported_primitives: usize,
}

impl RenderResult {
    pub fn from_frame(frame: &Frame, capabilities: RenderCapabilities) -> Self {
        let mut result = Self::default();
        for item in frame.scene.items() {
            if capabilities.supports(&item.primitive) {
                result.rendered_primitives += 1;
            } else {
                result.unsupported_primitives += 1;
            }
        }
        result
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderError {
    message: String,
}

impl RenderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RenderError {}

pub trait Renderer {
    fn capabilities(&self) -> RenderCapabilities;
    fn render(&mut self, frame: &Frame) -> Result<RenderResult, RenderError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{Color, Scene, Size, TextStyle};

    #[test]
    fn render_result_counts_unsupported_primitives() {
        let mut scene = Scene::new();
        scene.clear_color(Color::BLACK);
        scene.text(
            crate::render::Point::new(0.0, 0.0),
            "hello",
            TextStyle::new(Color::WHITE, 12.0),
        );
        let frame = Frame::new(Size::new(10.0, 10.0), scene);
        let capabilities = RenderCapabilities {
            fills: true,
            strokes: false,
            lines: false,
            text: false,
        };

        assert_eq!(
            RenderResult::from_frame(&frame, capabilities),
            RenderResult {
                rendered_primitives: 1,
                unsupported_primitives: 1,
            }
        );
    }

    #[test]
    fn render_error_preserves_message() {
        let error = RenderError::new("device lost");
        assert_eq!(error.message(), "device lost");
        assert_eq!(error.to_string(), "device lost");
    }
}
