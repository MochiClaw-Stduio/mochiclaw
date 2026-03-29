//! Mochiclaw Configuration
//!
//! Configuration structures shared across the Mochiclaw workspace.

mod agent;
mod channel;
mod error;
mod lambda;
mod model;
mod runtime;

use config::{
    Config as ConfigLoader, ConfigError as ConfigMgrError, Environment, File, FileFormat,
};
use std::collections::HashMap;
use std::path::Path;

pub use agent::AgentConfig;
pub use channel::ChannelConfig;
pub use error::ConfigError;
pub use lambda::{
    FsCapabilitiesOverride, LambdaCapabilitiesOverride, LambdaConfig, NetworkCapabilitiesOverride,
};
pub use model::ModelConfig;
pub use runtime::{LogConfig, NetworkConfig, RuntimeConfig};

/// Current configuration version
pub const CONFIG_VERSION: u32 = 1;

/// Root configuration structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// Configuration version for migration support
    #[serde(default, rename = "version")]
    pub version: Option<u32>,
    pub agent: AgentConfig,
    /// Runtime configuration (logging, retention, etc.)
    #[serde(default)]
    pub runtime: RuntimeConfig,
    /// Per-lambda configuration (key = lambda name)
    #[serde(default)]
    pub lambdas: HashMap<String, LambdaConfig>,
    /// Channel-specific lambda configuration
    #[serde(default)]
    pub channels: HashMap<String, ChannelConfig>,
    /// Model configurations
    #[serde(default)]
    pub models: HashMap<String, ModelConfig>,
}

impl Config {
    /// Load configuration from a TOML file with environment variable overrides.
    /// Environment variables with prefix `MOCHICLAW_` take priority over config file values.
    /// Use double underscore `__` for nesting: e.g., `MOCHICLAW_AGENT__MODEL`
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let path_str = path.to_string_lossy();
        let config = ConfigLoader::builder()
            .add_source(File::new(&path_str, FileFormat::Toml).required(true))
            .add_source(Environment::with_prefix("MOCHICLAW").separator("__"))
            .build()
            .map_err(|e: ConfigMgrError| {
                ConfigError::Parse(format!("failed to build config: {}", e))
            })?;

        let mut config: Config = config
            .try_deserialize()
            .map_err(|e| ConfigError::Parse(format!("failed to deserialize config: {}", e)))?;

        config.migrate()?;
        Ok(config)
    }

    /// Migrate configuration to current version
    fn migrate(&mut self) -> Result<(), ConfigError> {
        let version = self.version.unwrap_or(0);
        if version > CONFIG_VERSION {
            return Err(ConfigError::Parse(format!(
                "unsupported config version: {} (maximum supported: {})",
                version, CONFIG_VERSION
            )));
        }
        // Migrate from version 0 (unversioned) to version 1
        if version < 1 {
            // Version 1 adds the version field - no data migration needed
            self.version = Some(1);
        }
        Ok(())
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
        let content = toml::to_string_pretty(self).map_err(|e| {
            ConfigError::Serialization(format!("failed to serialize config: {}", e))
        })?;
        std::fs::write(path, content)
            .map_err(|e| ConfigError::Io(format!("failed to write {}: {}", path.display(), e)))
    }

    /// Create a default configuration for onboarding
    pub fn default_for_onboarding() -> Self {
        Self {
            version: Some(CONFIG_VERSION),
            agent: AgentConfig {
                model: "gpt-4".to_string(),
                max_iterations: 40,
                workspace: ".".to_string(),
            },
            runtime: RuntimeConfig {
                lambda_dirs: vec!["./lambdas".to_string()],
                log: LogConfig {
                    level: "info".to_string(),
                    dir: None,
                    max_age_days: None,
                },
                network: NetworkConfig {
                    use_system_proxy: false,
                },
            },
            lambdas: HashMap::new(),
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
