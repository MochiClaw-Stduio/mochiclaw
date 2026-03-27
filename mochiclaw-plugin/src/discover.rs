//! Plugin discovery - scans directories for plugins

use crate::manifest::PluginManifest;
use std::path::{Path, PathBuf};

/// Discovered plugin info
#[derive(Debug)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub manifest: PluginManifest,
    pub wasm_path: PathBuf,
}

/// Discover plugins from a directory
///
/// Scans subdirectories of the given path, looking for:
/// - A `manifest.toml` file to read plugin metadata
/// - A corresponding `.wasm` file in the target directory
///
/// Returns a list of discovered plugins that have `features.provider` or `features.channel` set.
pub fn discover(plugin_dir: &Path) -> Result<Vec<DiscoveredPlugin>, std::io::Error> {
    let mut plugins = Vec::new();

    if !plugin_dir.exists() {
        tracing::warn!("plugin directory does not exist: {}", plugin_dir.display());
        return Ok(plugins);
    }

    // Scan subdirectories for plugins
    for entry in std::fs::read_dir(plugin_dir)? {
        let entry = entry?;
        let entry_path = entry.path();

        if !entry_path.is_dir() {
            continue;
        }

        let plugin_name = match entry_path
            .file_name()
            .and_then(|n| n.to_str())
        {
            Some(n) => n,
            None => continue,
        };

        // Check for manifest.toml
        let manifest_path = entry_path.join("manifest.toml");
        let manifest = match PluginManifest::from_file(&manifest_path) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("failed to load manifest for '{}': {}", plugin_name, e);
                continue;
            }
        };

        // Check if this plugin should be loaded
        if !manifest.features.provider && !manifest.features.channel && !manifest.features.command {
            tracing::debug!("skipping '{}': not a channel, provider, or command", plugin_name);
            continue;
        }

        // Find the wasm file (same directory as manifest.toml)
        let wasm_path = entry_path.join(format!("{}.wasm", plugin_name));

        if !wasm_path.exists() {
            tracing::debug!("skipping '{}': no wasm found at {}", plugin_name, wasm_path.display());
            continue;
        }

        plugins.push(DiscoveredPlugin {
            name: plugin_name.to_string(),
            manifest,
            wasm_path,
        });
    }

    Ok(plugins)
}
