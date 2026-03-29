//! Command types for plugin commands

use serde::{Deserialize, Serialize};

/// Command info - metadata about a registered command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    /// Command name (e.g., "weather", "translate")
    pub name: String,
    /// Short description of the command
    pub description: String,
}

/// Request passed to a command plugin's execute function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRequest {
    /// Command name being invoked
    pub command: String,
    /// Arguments passed to the command
    pub args: String,
    /// Channel where the command was invoked
    pub channel: String,
    /// Chat ID where the command was invoked
    pub chat_id: String,
}

/// Response from a command plugin's execute function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    /// Whether the command executed successfully
    pub success: bool,
    /// Response text to send back to the user
    #[serde(default)]
    pub content: String,
    /// Error message if execution failed
    #[serde(default)]
    pub error: Option<String>,
}
