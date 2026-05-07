use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageLog {
    messages: Vec<Message>,
}

impl MessageLog {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    pub fn push(&mut self, level: MessageLevel, text: impl Into<String>) {
        self.messages.push(Message::new(level, text));
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.push(MessageLevel::Info, text);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.push(MessageLevel::Warning, text);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(MessageLevel::Error, text);
    }

    pub fn entries(&self) -> &[Message] {
        &self.messages
    }
}

impl Default for MessageLog {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Message {
    level: MessageLevel,
    text: String,
}

impl Message {
    pub fn new(level: MessageLevel, text: impl Into<String>) -> Self {
        Self {
            level,
            text: text.into(),
        }
    }

    pub fn level(&self) -> MessageLevel {
        self.level
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MessageLevel {
    Info,
    Warning,
    Error,
}
