//! Agent configuration

use serde::Deserialize;
use serde::Serialize;

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Default model name to use (references a model in [models])
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    /// Workspace directory for sessions and other data
    #[serde(default = "default_workspace")]
    pub workspace: String,
}

fn default_model() -> String {
    "gpt-4".to_string()
}

fn default_max_iterations() -> usize {
    40
}

fn default_workspace() -> String {
    ".".to_string()
}
