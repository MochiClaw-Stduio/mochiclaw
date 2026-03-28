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
    /// Capability overrides - merged with manifest capabilities
    /// User config takes precedence for scalars, lists are merged (user items appended)
    #[serde(default)]
    pub capabilities: Option<CapabilitiesOverride>,
}

fn default_enabled() -> bool {
    true
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            proxy_url: None,
            capabilities: None,
        }
    }
}

/// User overrides for manifest capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitiesOverride {
    #[serde(default)]
    pub network: Option<NetworkCapabilitiesOverride>,
    #[serde(default)]
    pub fs: Option<FsCapabilitiesOverride>,
    /// Appended to manifest's allowed_kv_read list
    #[serde(default)]
    pub allowed_kv_read: Option<Vec<String>>,
}

/// User overrides for network capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkCapabilitiesOverride {
    /// Override network.enabled
    pub enabled: Option<bool>,
    /// Appended to manifest's allowed_hosts list
    #[serde(default)]
    pub allowed_hosts: Option<Vec<String>>,
    /// Appended to manifest's denied_hosts list (takes precedence over allowed_hosts)
    #[serde(default)]
    pub denied_hosts: Option<Vec<String>>,
}

/// User overrides for filesystem capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FsCapabilitiesOverride {
    /// Override fs.enabled
    pub enabled: Option<bool>,
    /// Override allowed_root
    #[serde(default)]
    pub allowed_root: Option<String>,
    /// Appended to manifest's read_whitelist
    #[serde(default)]
    pub read_whitelist: Option<Vec<String>>,
    /// Appended to manifest's write_whitelist
    #[serde(default)]
    pub write_whitelist: Option<Vec<String>>,
    /// Appended to manifest's read_blacklist
    #[serde(default)]
    pub read_blacklist: Option<Vec<String>>,
    /// Appended to manifest's write_blacklist
    #[serde(default)]
    pub write_blacklist: Option<Vec<String>>,
}
