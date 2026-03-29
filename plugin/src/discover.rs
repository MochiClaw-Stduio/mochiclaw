//! Plugin discovery - scans directories for plugins

use crate::manifest::PluginManifest;
use std::path::{Path, PathBuf};

/// Discovered plugin info
#[derive(Debug)]
pub struct DiscoveredPlugin {
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

        // Check for manifest.toml
        let manifest_path = entry_path.join("manifest.toml");
        let manifest = match PluginManifest::from_file(&manifest_path) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    "failed to load manifest at '{}': {}",
                    manifest_path.display(),
                    e
                );
                continue;
            }
        };

        // Check if this plugin should be loaded
        if !manifest.features.provider
            && !manifest.features.channel
            && !manifest.features.command
            && !manifest.features.tool
        {
            tracing::debug!(
                "skipping '{}': not a channel, provider, command, or tool",
                manifest.name
            );
            continue;
        }

        // Find the wasm file (same directory as manifest.toml, named after manifest.name)
        let wasm_path = entry_path.join(format!("{}.wasm", manifest.name));

        if !wasm_path.exists() {
            tracing::debug!(
                "skipping '{}': no wasm found at {}",
                manifest.name,
                wasm_path.display()
            );
            continue;
        }

        plugins.push(DiscoveredPlugin {
            manifest,
            wasm_path,
        });
    }

    Ok(plugins)
}
