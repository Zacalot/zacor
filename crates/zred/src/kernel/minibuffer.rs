#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinibufferMode {
    Message,
    Command,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Minibuffer {
    mode: MinibufferMode,
    input: String,
}

impl Minibuffer {
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            mode: MinibufferMode::Message,
            input: message.into(),
        }
    }

    pub fn command() -> Self {
        Self {
            mode: MinibufferMode::Command,
            input: String::new(),
        }
    }

    pub fn mode(&self) -> MinibufferMode {
        self.mode
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn input_mut(&mut self) -> &mut String {
        &mut self.input
    }
}
