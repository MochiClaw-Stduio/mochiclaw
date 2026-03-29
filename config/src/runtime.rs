//! Runtime configuration

use serde::Deserialize;
use serde::Serialize;

/// Log configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogConfig {
    /// Log level (default: "info")
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Log release directory (absolute or relative to config file location)
    #[serde(default)]
    pub dir: Option<String>,
    /// Maximum age for log files in days (auto-cleanup)
    #[serde(default)]
    pub max_age_days: Option<u32>,
}

/// Network configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Whether to use system HTTP_PROXY environment variable as fallback
    #[serde(default)]
    pub use_system_proxy: bool,
}

/// Runtime configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Directories to scan for lambdas
    #[serde(default)]
    pub lambda_dirs: Vec<String>,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub network: NetworkConfig,
}

fn default_log_level() -> String {
    "info".to_string()
}
