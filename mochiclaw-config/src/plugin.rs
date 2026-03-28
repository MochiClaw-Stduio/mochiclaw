//! Plugin configuration

use serde::Deserialize;
use serde::Serialize;

/// Plugin configuration for all plugins (公共配置)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Per-plugin HTTP proxy URL (overrides global HTTP_PROXY env var)
    #[serde(default)]
    pub proxy_url: Option<String>,
}

fn default_enabled() -> bool {
    true
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            proxy_url: None,
        }
    }
}
