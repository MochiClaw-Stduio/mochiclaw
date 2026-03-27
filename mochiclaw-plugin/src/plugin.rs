//! Plugin Host - manages extism WASM plugins

use crate::discover::DiscoveredPlugin;
use crate::error::Error;
use crate::host::host_functions;
use crate::manifest::PluginManifest;
use extism::{Manifest, Wasm};
use extism::{Plugin, PluginBuilder};
use std::collections::HashMap;
use std::path::Path;

pub struct PluginHost {
    plugins: HashMap<String, Plugin>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Load a plugin with its manifest
    pub fn load_plugin(
        &mut self,
        name: &str,
        wasm_path: &Path,
        manifest: &PluginManifest,
    ) -> Result<(), Error> {
        if self.plugins.contains_key(name) {
            return Err(Error::Plugin(format!("plugin '{}' already loaded", name)));
        }

        let wasm_bytes = std::fs::read(wasm_path)
            .map_err(|e| Error::Plugin(format!("failed to read {}: {}", wasm_path.display(), e)))?;

        // Build extism Manifest with allowed hosts
        let extism_manifest = Manifest::new([Wasm::Data {
            data: wasm_bytes,
            meta: Default::default(),
        }])
        .with_allowed_hosts(manifest.capabilities.allowed_hosts.iter().cloned());

        // Create host functions
        let host_functions = host_functions();

        // Load plugin with capabilities from manifest
        tracing::debug!(
            "registering {} host functions for plugin '{}'",
            host_functions.len(),
            name
        );
        for f in &host_functions {
            tracing::debug!(
                "  - function: name={}, namespace={:?}",
                f.name(),
                f.namespace()
            );
        }

        let plugin = PluginBuilder::new(extism_manifest)
            .with_wasi(false)
            .with_functions(host_functions)
            .build()
            .map_err(|e| Error::Plugin(format!("failed to load plugin '{}': {}", name, e)))?;

        self.plugins.insert(name.to_string(), plugin);
        tracing::info!(
            "loaded plugin '{}' from {} (hosts: {:?})",
            name,
            wasm_path.display(),
            manifest.capabilities.allowed_hosts
        );
        Ok(())
    }

    pub fn call(&mut self, name: &str, function: &str, input: &str) -> Result<String, Error> {
        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| Error::Plugin(format!("plugin '{}' not found", name)))?;

        let output = plugin
            .call(function, input)
            .map_err(|e| Error::Plugin(format!("call failed: {}", e)))?;

        String::from_utf8(output).map_err(|e| Error::Plugin(format!("invalid UTF-8: {}", e)))
    }

    pub fn has_plugin(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Load a discovered plugin
    pub fn load_discovered(&mut self, plugin: DiscoveredPlugin) -> Result<(), Error> {
        self.load_plugin(&plugin.name, &plugin.wasm_path, &plugin.manifest)
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}
