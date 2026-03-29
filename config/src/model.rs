//! Model configuration

use serde::Deserialize;
use serde::Serialize;

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// The actual model name (e.g., "gpt-4", "claude-3-sonnet")
    pub model: String,
    /// Provider plugin to use (e.g., "mochiclaw-openai")
    pub provider: String,
    /// API endpoint base URL
    #[serde(default)]
    pub api_base: Option<String>,
    /// API key for the provider
    #[serde(default)]
    pub api_key: Option<String>,
}
