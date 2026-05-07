use super::registry::CommandRegistry;
use super::types::CommandRequest;

impl CommandRegistry {
    pub fn parse(&self, input: &str) -> CommandRequest {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return CommandRequest::Status("Ready".to_string());
        }

        let (name, args) = match trimmed.split_once(' ') {
            Some((name, args)) => (name, Some(args)),
            None => (trimmed, None),
        };

        match self.get(name) {
            Some(command) => command.parse(trimmed, args),
            None => CommandRequest::Status(format!("Unknown command: :{trimmed}")),
        }
    }
}
