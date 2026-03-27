//! Configuration management with serde support

use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub agent: AgentConfig,
    pub plugins: PluginsConfig,
    #[serde(default)]
    pub channels: HashMap<String, ChannelConfig>,
    #[serde(default)]
    pub models: HashMap<String, ModelConfig>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginsConfig {
    #[serde(default)]
    pub plugin_dirs: Vec<PluginDirConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDirConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

fn default_enabled() -> bool {
    true
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extra: HashMap::new(),
        }
    }
}

impl Config {
    pub fn from_file(path: &Path) -> Result<Self, Error> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("failed to read {}: {}", path.display(), e)))?;
        Self::from_toml(&content)
    }

    /// Get workspace path resolved relative to config file location
    pub fn workspace_path(&self, config_path: &Path) -> std::path::PathBuf {
        let workspace = std::path::Path::new(&self.agent.workspace);
        if workspace.is_absolute() {
            workspace.to_path_buf()
        } else {
            // Resolve relative to config file's directory
            config_path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join(workspace)
        }
    }

    pub fn from_toml(toml: &str) -> Result<Self, Error> {
        toml::from_str(toml).map_err(|e| Error::Config(format!("failed to parse config: {}", e)))
    }

    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("failed to serialize config: {}", e)))?;
        std::fs::write(path, content)
            .map_err(|e| Error::Config(format!("failed to write {}: {}", path.display(), e)))
    }
}
