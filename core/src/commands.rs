//! Slash command system - built-in commands and command registry
//!
//! Two registration mechanisms:
//! - Built-in commands: registered via `register_builtin` in `CommandRegistry`
//! - Plugin commands: called via `PluginHost::call` in AgentLoop (WASM-based)

use std::collections::HashMap;
use std::sync::Arc;

/// Parse command from message content, returns (command_name, args) or None
pub fn parse_command(content: &str) -> Option<(String, String)> {
    if !content.starts_with('/') {
        return None;
    }
    let rest = &content[1..];
    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    let name = parts[0].to_lowercase();
    let args = parts.get(1).unwrap_or(&"").to_string();
    Some((name, args))
}

/// Command metadata
pub struct CommandInfo {
    /// Short description of the command
    pub description: String,
}

impl CommandInfo {
    pub fn new(description: &str) -> Self {
        Self {
            description: description.to_string(),
        }
    }
}

/// Command function type: (channel, chat_id, args) -> optional response
type CommandFn = Arc<dyn Fn(&str, &str, &str) -> Option<String> + Send + Sync>;

/// Command registry for built-in commands
pub struct CommandRegistry {
    commands: HashMap<String, (CommandFn, CommandInfo)>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    /// Create a new empty command registry
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Register a built-in command handler with description
    /// Handler receives (channel, chat_id, args) and returns Some(response) or None
    pub fn register<F>(&mut self, name: &str, description: &str, handler: F)
    where
        F: Fn(&str, &str, &str) -> Option<String> + Send + Sync + 'static,
    {
        self.commands.insert(
            name.to_lowercase(),
            (Arc::new(handler), CommandInfo::new(description)),
        );
    }

    /// Execute a built-in command by name
    pub fn execute(&self, channel: &str, chat_id: &str, name: &str, args: &str) -> Option<String> {
        self.commands
            .get(name)
            .and_then(|(h, _)| h(channel, chat_id, args))
    }

    /// List all registered commands with their descriptions (sorted by name)
    pub fn list_commands(&self) -> Vec<(&str, &str)> {
        let mut commands: Vec<_> = self
            .commands
            .iter()
            .map(|(name, (_, info))| (name.as_str(), info.description.as_str()))
            .collect();
        commands.sort_by_key(|(name, _)| *name);
        commands
    }

    /// Generate help text listing all available commands
    pub fn generate_help(&self) -> String {
        let mut lines = vec!["Available commands:".to_string()];

        // Built-in commands always shown first
        lines.push(format!("/{} - {}", "help", "Show available commands"));
        lines.push(format!("/{} - {}", "clear", "Clear conversation history"));

        // Registered commands from plugins
        for (name, desc) in self.list_commands() {
            if name != "help" && name != "clear" {
                lines.push(format!("/{} - {}", name, desc));
            }
        }

        lines.push(String::new());
        lines.push("Use /<command> to invoke a command.".to_string());
        lines.join("\n")
    }

    /// Check if a command name is registered
    pub fn has_command(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command() {
        assert_eq!(
            parse_command("/help"),
            Some(("help".to_string(), "".to_string()))
        );
        assert_eq!(
            parse_command("/clear foo"),
            Some(("clear".to_string(), "foo".to_string()))
        );
        assert_eq!(
            parse_command("/foo bar baz"),
            Some(("foo".to_string(), "bar baz".to_string()))
        );
        assert_eq!(parse_command("help"), None);
        assert_eq!(parse_command(""), None);
    }

    #[test]
    fn test_registry() {
        let mut registry = CommandRegistry::new();
        registry.register("foo", "foo description", |_, _, _| Some("bar".to_string()));
        assert!(registry.has_command("foo"));
        assert!(!registry.has_command("unknown"));
        assert_eq!(
            registry.execute("c", "u", "foo", ""),
            Some("bar".to_string())
        );
        assert_eq!(registry.execute("c", "u", "unknown", ""), None);
    }

    #[test]
    fn test_generate_help() {
        let mut registry = CommandRegistry::new();
        registry.register("help", "Show available commands", |_, _, _| None);
        registry.register("clear", "Clear conversation history", |_, _, _| None);
        let help = registry.generate_help();
        assert!(help.contains("/help - Show available commands"));
        assert!(help.contains("/clear - Clear conversation history"));
    }
}
