use super::metadata::{Command, CommandMetadata};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandRegistry {
    commands: BTreeMap<String, Command>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, command: Command) -> Option<Command> {
        self.commands.insert(command.name().to_string(), command)
    }

    pub fn get(&self, name: &str) -> Option<&Command> {
        self.commands.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }

    pub fn entries(&self) -> impl Iterator<Item = CommandMetadata<'_>> {
        self.commands.values().map(Command::metadata)
    }

    pub fn entry(&self, name: &str) -> Option<CommandMetadata<'_>> {
        self.get(name).map(Command::metadata)
    }

    #[allow(dead_code)]
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.commands.keys().map(String::as_str)
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}
