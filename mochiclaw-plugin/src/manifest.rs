//! Plugin manifest schema

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use crate::error::Error as PluginError;

/// Plugin manifest - declares plugin metadata and capabilities
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    /// Plugin name (e.g., "mochiclaw-weixin")
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Human-readable description
    pub description: Option<String>,
    /// Plugin runtime capabilities and permissions
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Plugin features provided by this plugin
    #[serde(default)]
    pub features: Features,
    /// Plugin-specific settings schema
    #[serde(default)]
    pub settings: PluginSettingsSpec,
}

/// Schema for plugin-specific settings declared in manifest
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PluginSettingsSpec {
    /// Human-readable description of what settings this plugin expects
    #[serde(default)]
    pub description: Option<String>,
    /// Example settings for documentation/validation
    #[serde(default)]
    pub example: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Capabilities {
    /// Allowed HTTP hosts for this plugin
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Allowed KV read access to other plugins' key-value stores
    /// Format: list of plugin names this plugin can read KV from
    /// Write access is always limited to own plugin KV only
    #[serde(default)]
    pub allowed_kv_read: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Features {
    /// Whether this plugin provides LLM provider capability
    #[serde(default)]
    pub provider: bool,
    /// Whether this plugin provides channel capability
    #[serde(default)]
    pub channel: bool,
    /// Whether this plugin provides command capability
    #[serde(default)]
    pub command: bool,
}

impl PluginManifest {
    /// Load manifest from a TOML file
    pub fn from_file(path: &Path) -> Result<Self, PluginError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| PluginError::Manifest(format!("failed to read {}: {}", path.display(), e)))?;

        toml::from_str(&content)
            .map_err(|e| PluginError::Manifest(format!("failed to parse {}: {}", path.display(), e)))
    }
}

#[derive(Debug)]
pub enum ManifestError {
    Io(String),
    Parse(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Io(s) => write!(f, "manifest IO error: {}", s),
            ManifestError::Parse(s) => write!(f, "manifest parse error: {}", s),
        }
    }
}

impl std::error::Error for ManifestError {}
