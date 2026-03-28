# Slash Commands

[简体中文](../zh/commands.md) | English

---

## Overview

Slash commands are prefixed with `/` and provide quick access to agent functionality. They are parsed from user messages and executed before normal message processing.

## Built-in Commands

### /help

Shows available commands and their descriptions.

```
/help
```

Output:
```
Available commands:
/help - Show available commands
/clear - Clear conversation history

Use /<command> to invoke a command.
```

### /clear

Clears the conversation history for the current session.

```
/clear
```

Arguments (optional):
- Session ID or "all" to clear all sessions

## Command Format

Commands are parsed from message content:

| Format | Example | Parsed As |
|--------|---------|-----------|
| `/command` | `/help` | `("help", "")` |
| `/command args` | `/clear foo` | `("clear", "foo")` |

Notes:
- Commands are case-insensitive (`/HELP` = `/help`)
- Only the first space after the command name separates command from args
- Messages not starting with `/` are treated as regular messages

## Command Registry

The `CommandRegistry` manages available commands:

```rust
pub struct CommandRegistry {
    commands: HashMap<String, (CommandFn, CommandInfo)>,
}

// Command function signature
type CommandFn = Arc<dyn Fn(&str, &str, &str) -> Option<String> + Send + Sync>;
//                                ^channel  ^chat_id  ^args     ^response
```

### Registration

```rust
let mut registry = CommandRegistry::new();

registry.register("mycmd", "Does something special", |channel, chat_id, args| {
    Some(format!("Ran mycmd with args: {} in {} for {}", args, channel, chat_id))
});
```

### Execution

```rust
if let Some(response) = registry.execute("weixin", "user123", "mycmd", "arg1 arg2") {
    // Send response back to user
}
```

## Plugin Commands

Commands can also be provided by plugins with `features.command = true` in their manifest:

```toml
[features]
command = true
```

Plugin commands are called via `PluginHost::call` with the `execute` function.

## Parsing

The `parse_command` function extracts commands from messages:

```rust
use mochiclaw_core::commands::parse_command;

assert_eq!(parse_command("/help"), Some(("help".to_string(), "".to_string())));
assert_eq!(parse_command("/clear foo"), Some(("clear".to_string(), "foo".to_string())));
assert_eq!(parse_command("hello"), None);
```

## Flow

```
User Input
    ↓
parse_command() → None? → Normal message processing
    ↓
CommandRegistry.execute()
    ↓
Built-in handler / Plugin call
    ↓
Response sent via channel
```
