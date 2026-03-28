//! Plugin Host - manages extism WASM plugins using Pool for concurrency
//!
//! This module provides concurrent plugin execution via extism's Pool mechanism.
//! Each plugin type gets its own Pool, allowing multiple instances to run simultaneously.

use crate::discover::DiscoveredPlugin;
use crate::error::Error;
use crate::host::http::HttpContext;
use crate::host::kv::PluginKV;
use crate::manifest::PluginManifest;
use extism::{CompiledPlugin, Manifest, Plugin, PluginBuilder, Pool, PoolBuilder, Wasm};
use extism_convert::{FromBytesOwned, ToBytes};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// PluginHost manages multiple plugin pools for concurrent execution
pub struct PluginHost {
    /// Compiled plugins (contains JIT compiled engine) - used to create new instances
    compiled: HashMap<String, CompiledPlugin>,
    /// Plugin pools for concurrent execution
    pools: HashMap<String, Pool>,
    /// KV store for plugin state
    kv: Arc<PluginKV>,
    /// HTTP proxy URL (optional)
    http_proxy_url: Option<String>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            compiled: HashMap::new(),
            pools: HashMap::new(),
            kv: Arc::new(PluginKV::new()),
            http_proxy_url: std::env::var("HTTP_PROXY").ok(),
        }
    }

    /// Set HTTP proxy URL
    pub fn with_http_proxy(mut self, proxy_url: Option<String>) -> Self {
        self.http_proxy_url = proxy_url;
        self
    }

    /// Load a plugin with its manifest
    pub fn load_plugin(
        &mut self,
        name: &str,
        wasm_path: &Path,
        manifest: &PluginManifest,
    ) -> Result<(), Error> {
        if self.compiled.contains_key(name) {
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

        // Create host functions (rand + KV + HTTP with proxy support)
        let http_context = HttpContext::new(
            self.http_proxy_url.clone(),
            manifest.capabilities.allowed_hosts.clone(),
        )
        .map_err(|e| Error::Plugin(format!("failed to create HTTP context: {}", e)))?;

        let host_funcs = crate::host::HostFunctionsBuilder::new()
            .with_kv(
                self.kv.clone(),
                name,
                manifest.capabilities.allowed_kv_read.clone(),
            )
            .with_http(http_context)
            .build();

        tracing::debug!(
            "registering {} host functions for plugin '{}'",
            host_funcs.len(),
            name
        );
        for f in &host_funcs {
            tracing::debug!(
                "  - function: name={}, namespace={:?}",
                f.name(),
                f.namespace()
            );
        }

        // Build plugin builder
        let builder = PluginBuilder::new(extism_manifest)
            .with_wasi(false)
            .with_functions(host_funcs);

        // Compile to get a CompiledPlugin (contains JIT engine)
        let compiled = builder
            .clone()
            .compile()
            .map_err(|e| Error::Plugin(format!("failed to compile plugin '{}': {}", name, e)))?;

        // Create pool for this plugin
        // Note: CompiledPlugin is Clone (contains shared JIT engine), so we clone for the factory
        let compiled_for_pool = compiled.clone();
        let pool = PoolBuilder::new()
            .with_max_instances(std::thread::available_parallelism().unwrap().into())
            .build(move || {
                Plugin::new_from_compiled(&compiled_for_pool)
                    .map_err(|e| anyhow::anyhow!("failed to create plugin instance: {}", e))
            });

        self.compiled.insert(name.to_string(), compiled);
        self.pools.insert(name.to_string(), pool);

        tracing::info!(
            "loaded plugin '{}' from {} (hosts: {:?})",
            name,
            wasm_path.display(),
            manifest.capabilities.allowed_hosts
        );
        Ok(())
    }

    /// Call a plugin function with typed input/output, handling MessagePack serialization automatically.
    ///
    /// This is the preferred method for calling plugin functions - it handles
    /// serialization of the input and deserialization of the output automatically.
    ///
    /// # Type Parameters
    /// * `T` - Input type that implements `ToBytes` (e.g., types with `#[encoding(Msgpack)]`)
    /// * `R` - Output type that implements `FromBytesOwned` (e.g., types with `#[encoding(Msgpack)]`)
    ///
    /// # Example
    /// ```ignore
    /// let request = ChatRequest { ... };
    /// let response: ChatResponse = host.call("openai", "chat", &request)?;
    /// ```
    pub fn call<'a, T: ToBytes<'a>, R: FromBytesOwned>(
        &self,
        name: &str,
        function: &str,
        input: &'a T,
    ) -> Result<R, Error> {
        let pool = self.pools.get(name)
            .ok_or_else(|| Error::Plugin(format!("plugin '{}' not found", name)))?;

        let timeout = Duration::from_secs(120);
        let mut plugin = pool.get(timeout)
            .map_err(|e| Error::Plugin(format!("pool get timeout: {}", e)))?
            .ok_or_else(|| Error::Plugin("pool get timeout".into()))?;

        plugin
            .call(function, input)
            .map_err(|e| Error::Plugin(format!("call failed: {}", e)))
    }

    pub fn has_plugin(&self, name: &str) -> bool {
        self.pools.contains_key(name)
    }

    pub fn plugin_count(&self) -> usize {
        self.pools.len()
    }

    /// Load a discovered plugin
    pub fn load_discovered(&mut self, plugin: DiscoveredPlugin) -> Result<(), Error> {
        self.load_plugin(&plugin.name, &plugin.wasm_path, &plugin.manifest)
    }

    /// Get a reference to the KV store
    pub fn kv(&self) -> Arc<PluginKV> {
        self.kv.clone()
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}
