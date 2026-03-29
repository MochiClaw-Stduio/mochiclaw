//! Plugin Host - manages extism WASM plugins using Pool for concurrency
//!
//! This module provides concurrent plugin execution via extism's Pool mechanism.
//! Each plugin type gets its own Pool, allowing multiple instances to run simultaneously.

use crate::discover::DiscoveredPlugin;
use crate::error::Error;
use crate::host::fs::FsContext;
use crate::host::http::HttpContext;
use crate::host::kv::PluginKV;
use crate::manifest::PluginManifest;
use extism::{CompiledPlugin, Manifest, Plugin, PluginBuilder, Pool, PoolBuilder, Wasm};
use extism_convert::{FromBytesOwned, ToBytes};
use mochiclaw_config::PluginConfig;
use mochiclaw_sdk::tool::{ToolExecutionRequest, ToolExecutionResponse};
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
    /// Fallback HTTP proxy URL (from HTTP_PROXY env var when use_system_proxy=true)
    fallback_proxy_url: Option<String>,
    /// Whether to use system proxy when fallback_proxy_url is None
    use_system_proxy: bool,
    /// Manifests for all loaded plugins
    manifests: HashMap<String, PluginManifest>,
    /// Workspace directory for fs plugins
    workspace: Option<String>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            compiled: HashMap::new(),
            pools: HashMap::new(),
            kv: Arc::new(PluginKV::new()),
            fallback_proxy_url: None,
            use_system_proxy: false,
            manifests: HashMap::new(),
            workspace: None,
        }
    }

    /// Set fallback HTTP proxy URL and whether to use system proxy
    pub fn with_http_proxy(mut self, proxy_url: Option<String>, use_system_proxy: bool) -> Self {
        self.fallback_proxy_url = proxy_url;
        self.use_system_proxy = use_system_proxy;
        self
    }

    /// Set the workspace directory for fs plugins
    pub fn with_workspace(mut self, workspace: String) -> Self {
        self.workspace = Some(workspace);
        self
    }

    /// Set workspace after construction
    pub fn set_workspace(&mut self, workspace: String) {
        self.workspace = Some(workspace);
    }

    /// Load a plugin with its manifest and per-plugin config
    ///
    /// User config capabilities override manifest capabilities:
    /// - Boolean/string scalars: user override takes precedence (if set)
    /// - Lists: user items are appended to manifest's list
    pub fn load_plugin(
        &mut self,
        name: &str,
        wasm_path: &Path,
        manifest: &PluginManifest,
        config: &PluginConfig,
    ) -> Result<(), Error> {
        if self.compiled.contains_key(name) {
            return Err(Error::Plugin(format!("plugin '{}' already loaded", name)));
        }

        // Merge manifest capabilities with user overrides
        let capabilities = match &config.capabilities {
            Some(overrides) => manifest.capabilities.merge_with(overrides),
            None => manifest.capabilities.clone(),
        };

        let wasm_bytes = std::fs::read(wasm_path)
            .map_err(|e| Error::Plugin(format!("failed to read {}: {}", wasm_path.display(), e)))?;

        // Build extism Manifest with allowed hosts
        let mut extism_manifest = Manifest::new([Wasm::Data {
            data: wasm_bytes,
            meta: Default::default(),
        }])
        .with_allowed_hosts(capabilities.network.allowed_hosts.iter().cloned());

        // Inject workspace config if set (for fs plugins)
        if let Some(ref workspace) = self.workspace {
            extism_manifest = extism_manifest.with_config_key("workspace", workspace.clone());
        }

        // Determine effective proxy: per-plugin proxy_url > fallback_proxy_url
        let effective_proxy = config
            .proxy_url
            .clone()
            .or_else(|| self.fallback_proxy_url.clone());
        tracing::debug!(
            "loading plugin '{}', effective_proxy={:?}",
            name,
            effective_proxy
        );

        // Build host functions builder (always includes rand + KV)
        let mut builder = crate::host::HostFunctionsBuilder::new().with_kv(
            self.kv.clone(),
            name,
            capabilities.allowed_kv_read.clone(),
        );

        // Add HTTP functions if network is enabled
        if capabilities.network.enabled {
            let http_context = HttpContext::new(
                effective_proxy,
                capabilities.network.allowed_hosts.clone(),
                capabilities.network.denied_hosts.clone(),
                self.use_system_proxy,
            )
            .map_err(|e| Error::Plugin(format!("failed to create HTTP context: {}", e)))?;
            builder = builder.with_http(http_context);
        }

        // Add FS functions if fs is enabled
        if capabilities.fs.enabled {
            let workspace = self.workspace.clone().unwrap_or_else(|| ".".to_string());

            // Resolve ${workspace} placeholder in allowed_root
            let allowed_root = if capabilities.fs.allowed_root.is_empty() {
                workspace.clone()
            } else {
                capabilities
                    .fs
                    .allowed_root
                    .replace("${workspace}", &workspace)
            };

            let fs_context = FsContext::new(
                allowed_root.into(),
                capabilities.fs.read_whitelist.clone(),
                capabilities.fs.write_whitelist.clone(),
                capabilities.fs.read_blacklist.clone(),
                capabilities.fs.write_blacklist.clone(),
            );
            builder = builder.with_fs(fs_context);
        }

        let host_funcs = builder.build();

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

        // Store the manifest for later inspection
        self.manifests.insert(name.to_string(), manifest.clone());

        tracing::info!(
            "loaded plugin '{}' from {} (network: {:?}, fs: {:?})",
            name,
            wasm_path.display(),
            capabilities.network.enabled,
            capabilities.fs.enabled
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
        let pool = self
            .pools
            .get(name)
            .ok_or_else(|| Error::Plugin(format!("plugin '{}' not found", name)))?;

        let timeout = Duration::from_secs(120);
        let mut plugin = pool
            .get(timeout)
            .map_err(|e| Error::Plugin(format!("pool get timeout: {}", e)))?
            .ok_or_else(|| Error::Plugin("pool get timeout".into()))?;

        plugin
            .call(function, input)
            .map_err(|e| Error::Plugin(format!("call failed: {}", e)))
    }

    /// Call a tool function on a plugin.
    ///
    /// This is a convenience method specifically for tool execution that constructs
    /// the ToolExecutionRequest internally and calls the plugin's `execute_tool` function.
    pub fn call_tool(
        &self,
        plugin_name: &str,
        tool_name: &str,
        arguments: &HashMap<String, serde_json::Value>,
    ) -> Result<ToolExecutionResponse, Error> {
        let request = ToolExecutionRequest {
            name: tool_name.to_string(),
            arguments: arguments.clone(),
        };
        self.call(plugin_name, "execute_tool", &request)
    }

    pub fn has_plugin(&self, name: &str) -> bool {
        self.pools.contains_key(name)
    }

    pub fn plugin_count(&self) -> usize {
        self.pools.len()
    }

    /// Get a list of all loaded plugin names
    pub fn plugin_names(&self) -> Vec<String> {
        self.pools.keys().cloned().collect()
    }

    /// Get a list of plugin names that declared features.tool = true
    pub fn tool_plugins(&self) -> Vec<String> {
        self.manifests
            .iter()
            .filter(|(_, m)| m.features.tool)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Load a discovered plugin with default config
    pub fn load_discovered(&mut self, plugin: DiscoveredPlugin) -> Result<(), Error> {
        self.load_plugin(
            &plugin.manifest.name,
            &plugin.wasm_path,
            &plugin.manifest,
            &PluginConfig::default(),
        )
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
