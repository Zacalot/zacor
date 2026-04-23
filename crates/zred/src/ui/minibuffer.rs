#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinibufferMode {
    Message,
    Command,
}

#[derive(Clone, Debug)]
pub struct Minibuffer {
    pub mode: MinibufferMode,
    pub input: String,
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
}
