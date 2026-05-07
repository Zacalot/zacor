use super::{Frame, RenderCapabilities, RenderError, RenderResult, Renderer};

#[derive(Debug, Default)]
pub struct TestRenderer {
    capabilities: RenderCapabilities,
    frames: Vec<Frame>,
}

impl TestRenderer {
    pub fn new(capabilities: RenderCapabilities) -> Self {
        Self {
            capabilities,
            frames: Vec::new(),
        }
    }

    pub fn with_all_capabilities() -> Self {
        Self::new(RenderCapabilities::all())
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }
}

impl Renderer for TestRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        self.capabilities
    }

    fn render(&mut self, frame: &Frame) -> Result<RenderResult, RenderError> {
        let result = RenderResult::from_frame(frame, self.capabilities);
        self.frames.push(frame.clone());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{Color, Rect, Scene, Size};

    #[test]
    fn records_submitted_frames_exactly() {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::from_xywh(1.0, 2.0, 3.0, 4.0), Color::WHITE);
        let frame = Frame::new(Size::new(20.0, 10.0), scene);

        let mut renderer = TestRenderer::with_all_capabilities();
        let result = renderer.render(&frame).unwrap();

        assert_eq!(
            result,
            RenderResult {
                rendered_primitives: 1,
                unsupported_primitives: 0,
            }
        );
        assert_eq!(renderer.frames(), &[frame]);
    }

    #[test]
    fn respects_configured_capabilities() {
        let mut scene = Scene::new();
        scene.fill_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        let frame = Frame::new(Size::new(1.0, 1.0), scene);

        let mut renderer = TestRenderer::new(RenderCapabilities::none());
        assert_eq!(
            renderer.render(&frame).unwrap(),
            RenderResult {
                rendered_primitives: 0,
                unsupported_primitives: 1,
            }
        );
    }
}
