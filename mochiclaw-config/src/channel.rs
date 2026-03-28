//! Channel plugin configuration

use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

/// Channel-specific plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Authentication token
    #[serde(default)]
    pub token: Option<String>,
    /// Plugin-specific extra settings (e.g., base_url, etc.)
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
            token: None,
            extra: HashMap::new(),
        }
    }
}
