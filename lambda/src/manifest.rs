//! Plugin manifest schema

use crate::error::Error as PluginError;
use mochiclaw_config::LambdaCapabilitiesOverride;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

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
    /// Network access capabilities
    #[serde(default)]
    pub network: NetworkCapabilities,
    /// Filesystem access capabilities
    #[serde(default)]
    pub fs: FsCapabilities,
    /// Allowed KV read access to other plugins' key-value stores
    /// Format: list of plugin names this plugin can read KV from
    /// Write access is always limited to own plugin KV only
    #[serde(default)]
    pub allowed_kv_read: Vec<String>,
}

impl Capabilities {
    /// Merge manifest capabilities with user overrides
    ///
    /// Rules:
    /// - Boolean scalars (enabled): user override takes precedence (if Some)
    /// - String scalars (allowed_root): user override takes precedence (if Some)
    /// - Lists: user items are appended to manifest's list
    pub fn merge_with(&self, overrides: &LambdaCapabilitiesOverride) -> Self {
        // Merge network capabilities
        let network_enabled = overrides
            .network
            .as_ref()
            .and_then(|n| n.enabled)
            .unwrap_or(self.network.enabled);
        let mut allowed_hosts = self.network.allowed_hosts.clone();
        let mut denied_hosts = self.network.denied_hosts.clone();
        if let Some(ref network_override) = overrides.network {
            if let Some(ref user_hosts) = network_override.allowed_hosts {
                allowed_hosts.extend(user_hosts.iter().cloned());
            }
            if let Some(ref user_denied) = network_override.denied_hosts {
                denied_hosts.extend(user_denied.iter().cloned());
            }
        }

        // Merge fs capabilities
        let fs_enabled = overrides
            .fs
            .as_ref()
            .and_then(|f| f.enabled)
            .unwrap_or(self.fs.enabled);
        let allowed_root = overrides
            .fs
            .as_ref()
            .and_then(|f| f.allowed_root.clone())
            .unwrap_or_else(|| self.fs.allowed_root.clone());

        let mut read_whitelist = self.fs.read_whitelist.clone();
        if let Some(ref fs_override) = overrides.fs
            && let Some(ref user_list) = fs_override.read_whitelist
        {
            read_whitelist.extend(user_list.iter().cloned());
        }

        let mut write_whitelist = self.fs.write_whitelist.clone();
        if let Some(ref fs_override) = overrides.fs
            && let Some(ref user_list) = fs_override.write_whitelist
        {
            write_whitelist.extend(user_list.iter().cloned());
        }

        let mut read_blacklist = self.fs.read_blacklist.clone();
        if let Some(ref fs_override) = overrides.fs
            && let Some(ref user_list) = fs_override.read_blacklist
        {
            read_blacklist.extend(user_list.iter().cloned());
        }

        let mut write_blacklist = self.fs.write_blacklist.clone();
        if let Some(ref fs_override) = overrides.fs
            && let Some(ref user_list) = fs_override.write_blacklist
        {
            write_blacklist.extend(user_list.iter().cloned());
        }

        // Merge allowed_kv_read
        let mut allowed_kv_read = self.allowed_kv_read.clone();
        if let Some(ref user_list) = overrides.allowed_kv_read {
            allowed_kv_read.extend(user_list.iter().cloned());
        }

        Self {
            network: NetworkCapabilities {
                enabled: network_enabled,
                allowed_hosts,
                denied_hosts,
            },
            fs: FsCapabilities {
                enabled: fs_enabled,
                allowed_root,
                read_whitelist,
                write_whitelist,
                read_blacklist,
                write_blacklist,
            },
            allowed_kv_read,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NetworkCapabilities {
    /// Whether network access is enabled
    #[serde(default = "default_false")]
    pub enabled: bool,
    /// Allowed HTTP hosts for this plugin (glob patterns supported)
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Denied HTTP hosts for this plugin (takes precedence over allowed_hosts, glob patterns supported)
    #[serde(default)]
    pub denied_hosts: Vec<String>,
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FsCapabilities {
    /// Whether filesystem access is enabled
    #[serde(default = "default_false")]
    pub enabled: bool,
    /// Allowed root directory for filesystem operations (sandbox boundary)
    /// Supports ${workspace} placeholder which is replaced at runtime
    #[serde(default)]
    pub allowed_root: String,
    /// Allowed filesystem read paths (absolute paths)
    /// If empty, read is allowed for all paths within allowed_root
    #[serde(default)]
    pub read_whitelist: Vec<String>,
    /// Allowed filesystem write paths (absolute paths)
    /// If empty, write is allowed for all paths within allowed_root
    #[serde(default)]
    pub write_whitelist: Vec<String>,
    /// Disallowed filesystem read paths (takes precedence over whitelist)
    #[serde(default)]
    pub read_blacklist: Vec<String>,
    /// Disallowed filesystem write paths (takes precedence over whitelist)
    #[serde(default)]
    pub write_blacklist: Vec<String>,
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
    /// Whether this plugin provides tool capability
    #[serde(default)]
    pub tool: bool,
}

impl PluginManifest {
    /// Load manifest from a TOML file
    pub fn from_file(path: &Path) -> Result<Self, PluginError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            PluginError::Manifest(format!("failed to read {}: {}", path.display(), e))
        })?;

        toml::from_str(&content).map_err(|e| {
            PluginError::Manifest(format!("failed to parse {}: {}", path.display(), e))
        })
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

#[cfg(test)]
mod tests {
    use crate::manifest::{Capabilities, FsCapabilities, NetworkCapabilities};
    use mochiclaw_config::{
        FsCapabilitiesOverride, LambdaCapabilitiesOverride, NetworkCapabilitiesOverride,
    };

    fn default_capabilities() -> Capabilities {
        Capabilities {
            network: NetworkCapabilities {
                enabled: true,
                allowed_hosts: vec!["api.manifest.com".to_string()],
                denied_hosts: vec!["evil.manifest.com".to_string()],
            },
            fs: FsCapabilities {
                enabled: true,
                allowed_root: "/manifest/root".to_string(),
                read_whitelist: vec!["/manifest/read".to_string()],
                write_whitelist: vec!["/manifest/write".to_string()],
                read_blacklist: vec![],
                write_blacklist: vec![],
            },
            allowed_kv_read: vec!["plugin-a".to_string()],
        }
    }

    // ==================== Network capabilities tests ====================

    #[test]
    fn test_merge_network_enabled_true_to_false() {
        // User disables network even though manifest has it enabled
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: Some(NetworkCapabilitiesOverride {
                enabled: Some(false),
                allowed_hosts: None,
                denied_hosts: None,
            }),
            fs: None,
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert!(!result.network.enabled);
        // allowed_hosts should remain unchanged since no override
        assert_eq!(result.network.allowed_hosts, vec!["api.manifest.com"]);
    }

    #[test]
    fn test_merge_network_enabled_false_to_true() {
        // User enables network even though manifest has it disabled
        let manifest = Capabilities {
            network: NetworkCapabilities {
                enabled: false,
                allowed_hosts: vec![],
                denied_hosts: vec![],
            },
            fs: Default::default(),
            allowed_kv_read: vec![],
        };
        let overrides = LambdaCapabilitiesOverride {
            network: Some(NetworkCapabilitiesOverride {
                enabled: Some(true),
                allowed_hosts: Some(vec!["user.api.com".to_string()]),
                denied_hosts: None,
            }),
            fs: None,
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert!(result.network.enabled);
        assert_eq!(result.network.allowed_hosts, vec!["user.api.com"]);
    }

    #[test]
    fn test_merge_network_allowed_hosts_append() {
        // User appends additional allowed hosts
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: Some(NetworkCapabilitiesOverride {
                enabled: None,
                allowed_hosts: Some(vec![
                    "user.api.com".to_string(),
                    "another.api.com".to_string(),
                ]),
                denied_hosts: None,
            }),
            fs: None,
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert!(result.network.enabled);
        assert_eq!(
            result.network.allowed_hosts,
            vec!["api.manifest.com", "user.api.com", "another.api.com",]
        );
    }

    #[test]
    fn test_merge_network_allowed_hosts_empty_append() {
        // User appends empty list - should not change
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: Some(NetworkCapabilitiesOverride {
                enabled: None,
                allowed_hosts: Some(vec![]),
                denied_hosts: None,
            }),
            fs: None,
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert_eq!(result.network.allowed_hosts, vec!["api.manifest.com"]);
    }

    #[test]
    fn test_merge_network_denied_hosts_append() {
        // User appends to denied_hosts
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: Some(NetworkCapabilitiesOverride {
                enabled: None,
                allowed_hosts: None,
                denied_hosts: Some(vec!["user.denied.com".to_string()]),
            }),
            fs: None,
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert_eq!(
            result.network.denied_hosts,
            vec!["evil.manifest.com", "user.denied.com"]
        );
    }

    #[test]
    fn test_merge_network_denied_hosts_empty_append() {
        // User appends empty denied_hosts - should not change
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: Some(NetworkCapabilitiesOverride {
                enabled: None,
                allowed_hosts: None,
                denied_hosts: Some(vec![]),
            }),
            fs: None,
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert_eq!(result.network.denied_hosts, vec!["evil.manifest.com"]);
    }

    // ==================== FS capabilities tests ====================

    #[test]
    fn test_merge_fs_enabled_true_to_false() {
        // User disables fs
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: None,
            fs: Some(FsCapabilitiesOverride {
                enabled: Some(false),
                allowed_root: None,
                read_whitelist: None,
                write_whitelist: None,
                read_blacklist: None,
                write_blacklist: None,
            }),
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert!(!result.fs.enabled);
    }

    #[test]
    fn test_merge_fs_enabled_false_to_true() {
        // User enables fs when manifest has it disabled
        let manifest = Capabilities {
            network: Default::default(),
            fs: FsCapabilities {
                enabled: false,
                allowed_root: String::new(),
                read_whitelist: vec![],
                write_whitelist: vec![],
                read_blacklist: vec![],
                write_blacklist: vec![],
            },
            allowed_kv_read: vec![],
        };
        let overrides = LambdaCapabilitiesOverride {
            network: None,
            fs: Some(FsCapabilitiesOverride {
                enabled: Some(true),
                allowed_root: Some("/user/root".to_string()),
                read_whitelist: None,
                write_whitelist: None,
                read_blacklist: None,
                write_blacklist: None,
            }),
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert!(result.fs.enabled);
        assert_eq!(result.fs.allowed_root, "/user/root");
    }

    #[test]
    fn test_merge_fs_allowed_root_override() {
        // User overrides allowed_root
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: None,
            fs: Some(FsCapabilitiesOverride {
                enabled: None,
                allowed_root: Some("/user/root".to_string()),
                read_whitelist: None,
                write_whitelist: None,
                read_blacklist: None,
                write_blacklist: None,
            }),
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert_eq!(result.fs.allowed_root, "/user/root");
    }

    #[test]
    fn test_merge_fs_whitelists_append() {
        // User appends to read and write whitelists
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: None,
            fs: Some(FsCapabilitiesOverride {
                enabled: None,
                allowed_root: None,
                read_whitelist: Some(vec!["/user/read".to_string()]),
                write_whitelist: Some(vec!["/user/write".to_string()]),
                read_blacklist: None,
                write_blacklist: None,
            }),
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert_eq!(
            result.fs.read_whitelist,
            vec!["/manifest/read", "/user/read"]
        );
        assert_eq!(
            result.fs.write_whitelist,
            vec!["/manifest/write", "/user/write"]
        );
    }

    #[test]
    fn test_merge_fs_blacklists_append() {
        // User appends to read and write blacklists
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: None,
            fs: Some(FsCapabilitiesOverride {
                enabled: None,
                allowed_root: None,
                read_whitelist: None,
                write_whitelist: None,
                read_blacklist: Some(vec!["/user/forbidden-read".to_string()]),
                write_blacklist: Some(vec!["/user/forbidden-write".to_string()]),
            }),
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert_eq!(result.fs.read_blacklist, vec!["/user/forbidden-read"]);
        assert_eq!(result.fs.write_blacklist, vec!["/user/forbidden-write"]);
    }

    // ==================== KV read capabilities tests ====================

    #[test]
    fn test_merge_allowed_kv_read_append() {
        // User appends additional KV read access
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: None,
            fs: None,
            allowed_kv_read: Some(vec!["plugin-b".to_string(), "plugin-c".to_string()]),
        };

        let result = manifest.merge_with(&overrides);

        assert_eq!(
            result.allowed_kv_read,
            vec!["plugin-a", "plugin-b", "plugin-c"]
        );
    }

    #[test]
    fn test_merge_allowed_kv_read_empty_append() {
        // User appends empty list
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: None,
            fs: None,
            allowed_kv_read: Some(vec![]),
        };

        let result = manifest.merge_with(&overrides);

        assert_eq!(result.allowed_kv_read, vec!["plugin-a"]);
    }

    // ==================== Combined/edge case tests ====================

    #[test]
    fn test_merge_all_fields_at_once() {
        // User overrides everything at once
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: Some(NetworkCapabilitiesOverride {
                enabled: Some(false),
                allowed_hosts: Some(vec!["new.host.com".to_string()]),
                denied_hosts: Some(vec!["new.denied.com".to_string()]),
            }),
            fs: Some(FsCapabilitiesOverride {
                enabled: Some(true),
                allowed_root: Some("/new/root".to_string()),
                read_whitelist: Some(vec!["/new/read".to_string()]),
                write_whitelist: Some(vec!["/new/write".to_string()]),
                read_blacklist: Some(vec!["/new/blacklist".to_string()]),
                write_blacklist: Some(vec!["/new/write-blacklist".to_string()]),
            }),
            allowed_kv_read: Some(vec!["new-plugin".to_string()]),
        };

        let result = manifest.merge_with(&overrides);

        // Network
        assert!(!result.network.enabled);
        assert_eq!(
            result.network.allowed_hosts,
            vec!["api.manifest.com", "new.host.com"]
        );
        assert_eq!(
            result.network.denied_hosts,
            vec!["evil.manifest.com", "new.denied.com"]
        );

        // FS
        assert!(result.fs.enabled);
        assert_eq!(result.fs.allowed_root, "/new/root");
        assert_eq!(
            result.fs.read_whitelist,
            vec!["/manifest/read", "/new/read"]
        );
        assert_eq!(
            result.fs.write_whitelist,
            vec!["/manifest/write", "/new/write"]
        );
        assert_eq!(result.fs.read_blacklist, vec!["/new/blacklist"]);
        assert_eq!(result.fs.write_blacklist, vec!["/new/write-blacklist"]);

        // KV
        assert_eq!(result.allowed_kv_read, vec!["plugin-a", "new-plugin"]);
    }

    #[test]
    fn test_merge_empty_overrides_keeps_manifest() {
        // Empty overrides should keep all manifest values
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride::default();

        let result = manifest.merge_with(&overrides);

        assert_eq!(result.network.enabled, manifest.network.enabled);
        assert_eq!(result.network.allowed_hosts, manifest.network.allowed_hosts);
        assert_eq!(result.fs.enabled, manifest.fs.enabled);
        assert_eq!(result.fs.allowed_root, manifest.fs.allowed_root);
        assert_eq!(result.fs.read_whitelist, manifest.fs.read_whitelist);
        assert_eq!(result.fs.write_whitelist, manifest.fs.write_whitelist);
        assert_eq!(result.fs.read_blacklist, manifest.fs.read_blacklist);
        assert_eq!(result.fs.write_blacklist, manifest.fs.write_blacklist);
        assert_eq!(result.allowed_kv_read, manifest.allowed_kv_read);
    }

    #[test]
    fn test_merge_partial_network_override() {
        // User only overrides network enabled, nothing else
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: Some(NetworkCapabilitiesOverride {
                enabled: Some(false),
                allowed_hosts: None,
                denied_hosts: None,
            }),
            fs: None,
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        // Network
        assert!(!result.network.enabled);
        assert_eq!(result.network.allowed_hosts, manifest.network.allowed_hosts);

        // FS unchanged
        assert_eq!(result.fs.enabled, manifest.fs.enabled);
        assert_eq!(result.fs.allowed_root, manifest.fs.allowed_root);
        assert_eq!(result.fs.read_whitelist, manifest.fs.read_whitelist);

        // KV unchanged
        assert_eq!(result.allowed_kv_read, manifest.allowed_kv_read);
    }

    #[test]
    fn test_merge_partial_fs_override() {
        // User only overrides fs allowed_root, no other fs fields
        let manifest = default_capabilities();
        let overrides = LambdaCapabilitiesOverride {
            network: None,
            fs: Some(FsCapabilitiesOverride {
                enabled: None,
                allowed_root: Some("/override/root".to_string()),
                read_whitelist: None,
                write_whitelist: None,
                read_blacklist: None,
                write_blacklist: None,
            }),
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        // FS only allowed_root changed
        assert_eq!(result.fs.allowed_root, "/override/root");
        assert!(result.fs.enabled); // unchanged
        assert_eq!(result.fs.read_whitelist, manifest.fs.read_whitelist); // unchanged
        assert_eq!(result.fs.write_whitelist, manifest.fs.write_whitelist); // unchanged

        // Network unchanged
        assert_eq!(result.network.enabled, manifest.network.enabled);
        assert_eq!(result.network.allowed_hosts, manifest.network.allowed_hosts);
    }

    #[test]
    fn test_merge_manifest_network_disabled_user_enables_with_hosts() {
        // Manifest has network disabled, user enables and specifies hosts
        let manifest = Capabilities {
            network: NetworkCapabilities {
                enabled: false,
                allowed_hosts: vec![],
                denied_hosts: vec![],
            },
            fs: Default::default(),
            allowed_kv_read: vec![],
        };
        let overrides = LambdaCapabilitiesOverride {
            network: Some(NetworkCapabilitiesOverride {
                enabled: Some(true),
                allowed_hosts: Some(vec!["*.user-specified.com".to_string()]),
                denied_hosts: None,
            }),
            fs: None,
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert!(result.network.enabled);
        assert_eq!(result.network.allowed_hosts, vec!["*.user-specified.com"]);
    }

    #[test]
    fn test_merge_manifest_fs_disabled_user_enables_with_restrictions() {
        // Manifest has fs disabled, user enables and restricts to specific paths
        let manifest = Capabilities {
            network: Default::default(),
            fs: FsCapabilities {
                enabled: false,
                allowed_root: String::new(),
                read_whitelist: vec![],
                write_whitelist: vec![],
                read_blacklist: vec![],
                write_blacklist: vec![],
            },
            allowed_kv_read: vec![],
        };
        let overrides = LambdaCapabilitiesOverride {
            network: None,
            fs: Some(FsCapabilitiesOverride {
                enabled: Some(true),
                allowed_root: Some("/restricted".to_string()),
                read_whitelist: Some(vec!["/restricted/readonly".to_string()]),
                write_whitelist: Some(vec!["/restricted/readonly".to_string()]),
                read_blacklist: None,
                write_blacklist: None,
            }),
            allowed_kv_read: None,
        };

        let result = manifest.merge_with(&overrides);

        assert!(result.fs.enabled);
        assert_eq!(result.fs.allowed_root, "/restricted");
        assert_eq!(result.fs.read_whitelist, vec!["/restricted/readonly"]);
        assert_eq!(result.fs.write_whitelist, vec!["/restricted/readonly"]);
    }
}
