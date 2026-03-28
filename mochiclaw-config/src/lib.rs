//! Mochiclaw Configuration
//!
//! Configuration structures shared across the Mochiclaw workspace.

mod agent;
mod channel;
mod error;
mod model;
mod plugin;

use std::collections::HashMap;
use std::path::Path;

pub use agent::AgentConfig;
pub use channel::ChannelConfig;
pub use error::ConfigError;
pub use model::ModelConfig;
pub use plugin::PluginConfig;

/// Root configuration structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub agent: AgentConfig,
    /// Per-plugin configuration (key = plugin name)
    #[serde(default)]
    pub plugins: HashMap<String, PluginConfig>,
    /// Channel-specific plugin configuration
    #[serde(default)]
    pub channels: HashMap<String, ChannelConfig>,
    /// Model configurations
    #[serde(default)]
    pub models: HashMap<String, ModelConfig>,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(format!("failed to read {}: {}", path.display(), e)))?;
        Self::from_toml(&content)
    }

    /// Parse configuration from TOML string
    pub fn from_toml(toml: &str) -> Result<Self, ConfigError> {
        toml::from_str(toml).map_err(|e| ConfigError::Parse(format!("failed to parse config: {}", e)))
    }

    /// Get workspace path resolved relative to config file location
    pub fn workspace_path(&self, config_path: &Path) -> std::path::PathBuf {
        let workspace = std::path::Path::new(&self.agent.workspace);
        if workspace.is_absolute() {
            workspace.to_path_buf()
        } else {
            config_path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join(workspace)
        }
    }

    /// Save configuration to a TOML file
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Serialization(format!("failed to serialize config: {}", e)))?;
        std::fs::write(path, content)
            .map_err(|e| ConfigError::Io(format!("failed to write {}: {}", path.display(), e)))
    }

    /// Create a default configuration for onboarding
    pub fn default_for_onboarding() -> Self {
        Self {
            agent: AgentConfig {
                model: "gpt-4".to_string(),
                max_iterations: 40,
                workspace: ".".to_string(),
                plugin_dirs: vec!["./plugins".to_string()],
                use_system_proxy: false,
            },
            plugins: HashMap::new(),
            channels: HashMap::new(),
            models: HashMap::from([(
                "gpt-4".to_string(),
                ModelConfig {
                    model: "gpt-4".to_string(),
                    provider: "mochiclaw-openai".to_string(),
                    api_base: None,
                    api_key: None,
                },
            )]),
        }
    }
}
