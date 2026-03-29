//! Lambda Host - manages extism WASM lambdas using Pool for concurrency
//!
//! This module provides concurrent lambda execution via extism's Pool mechanism.
//! Each lambda type gets its own Pool, allowing multiple instances to run simultaneously.

use crate::context::{LambdaContext, LambdaContextMap};
use crate::discover::DiscoveredLambda;
use crate::error::Error;
use crate::host::fs::FsContext;
use crate::host::kv::LambdaKV;
use crate::manifest::LambdaManifest;
use extism::{CompiledPlugin, Manifest, Plugin, PluginBuilder, Pool, PoolBuilder, Wasm};
use extism_convert::{FromBytesOwned, ToBytes};
use mochiclaw_config::LambdaConfig;
use mochiclaw_sdk::tool::{ToolExecutionRequest, ToolExecutionResponse};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// LambdaHost manages multiple lambda pools for concurrent execution
pub struct LambdaHost {
    /// Compiled lambdas (contains JIT compiled engine) - used to create new instances
    compiled: HashMap<String, CompiledPlugin>,
    /// Lambda pools for concurrent execution
    pools: HashMap<String, Pool>,
    /// KV store for lambda state
    kv: Arc<LambdaKV>,
    /// Fallback HTTP proxy URL (from HTTP_PROXY env var when use_system_proxy=true)
    fallback_proxy_url: Option<String>,
    /// Whether to use system proxy when fallback_proxy_url is None
    use_system_proxy: bool,
    /// Lambda contexts (manifest + config merged)
    contexts: LambdaContextMap,
    /// Workspace directory for fs lambdas
    workspace: Option<String>,
}

impl LambdaHost {
    pub fn new() -> Self {
        Self {
            compiled: HashMap::new(),
            pools: HashMap::new(),
            kv: Arc::new(LambdaKV::new()),
            fallback_proxy_url: None,
            use_system_proxy: false,
            contexts: LambdaContextMap::new(),
            workspace: None,
        }
    }

    /// Set fallback HTTP proxy URL and whether to use system proxy
    pub fn with_http_proxy(mut self, proxy_url: Option<String>, use_system_proxy: bool) -> Self {
        self.fallback_proxy_url = proxy_url;
        self.use_system_proxy = use_system_proxy;
        self
    }

    /// Set the workspace directory for fs lambdas
    pub fn with_workspace(mut self, workspace: String) -> Self {
        self.workspace = Some(workspace);
        self
    }

    /// Set workspace after construction
    pub fn set_workspace(&mut self, workspace: String) {
        self.workspace = Some(workspace);
    }

    /// Load a lambda with its manifest and per-lambda config
    ///
    /// User config capabilities override manifest capabilities:
    /// - Boolean/string scalars: user override takes precedence (if set)
    /// - Lists: user items are appended to manifest's list
    pub fn load_lambda(
        &mut self,
        name: &str,
        wasm_path: &Path,
        manifest: &LambdaManifest,
        config: &LambdaConfig,
    ) -> Result<(), Error> {
        if self.compiled.contains_key(name) {
            return Err(Error::Lambda(format!("lambda '{}' already loaded", name)));
        }

        // Merge manifest capabilities with user overrides
        let capabilities = match &config.capabilities {
            Some(overrides) => manifest.capabilities.merge_with(overrides),
            None => manifest.capabilities.clone(),
        };

        let wasm_bytes = std::fs::read(wasm_path)
            .map_err(|e| Error::Lambda(format!("failed to read {}: {}", wasm_path.display(), e)))?;

        // Build extism Manifest with allowed hosts
        let mut extism_manifest = Manifest::new([Wasm::Data {
            data: wasm_bytes,
            meta: Default::default(),
        }])
        .with_allowed_hosts(capabilities.network.allowed_hosts.iter().cloned());

        // Inject workspace config if set (for fs lambdas)
        if let Some(ref workspace) = self.workspace {
            extism_manifest = extism_manifest.with_config_key("workspace", workspace.clone());
        }

        // Determine effective proxy: per-lambda proxy_url > fallback_proxy_url
        let effective_proxy = config
            .proxy_url
            .clone()
            .or_else(|| self.fallback_proxy_url.clone());
        tracing::debug!(
            "loading lambda '{}', effective_proxy={:?}",
            name,
            effective_proxy
        );

        // Build host functions builder (always includes rand + KV)
        let mut builder = crate::host::HostFunctionsBuilder::new().with_kv(
            self.kv.clone(),
            name,
            capabilities.allowed_kv_read.clone(),
        );

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

            // Resolve ${workspace} placeholder in whitelists/blacklists
            let resolve_list = |list: &[String]| -> Vec<String> {
                list.iter()
                    .map(|p| {
                        if p.is_empty() {
                            workspace.clone()
                        } else {
                            p.replace("${workspace}", &workspace)
                        }
                    })
                    .collect()
            };

            let fs_context = FsContext::new(
                allowed_root.into(),
                resolve_list(&capabilities.fs.read_whitelist),
                resolve_list(&capabilities.fs.write_whitelist),
                resolve_list(&capabilities.fs.read_blacklist),
                resolve_list(&capabilities.fs.write_blacklist),
            );
            builder = builder.with_fs(fs_context);
        }

        let host_funcs = builder.build();

        tracing::debug!(
            "registering {} host functions for lambda '{}'",
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

        // Build lambda builder
        let builder = PluginBuilder::new(extism_manifest)
            .with_wasi(false)
            .with_functions(host_funcs);

        // Compile to get a CompiledPlugin (contains JIT engine)
        let compiled = builder
            .clone()
            .compile()
            .map_err(|e| Error::Lambda(format!("failed to compile lambda '{}': {}", name, e)))?;

        // Create pool for this lambda
        // Note: CompiledPlugin is Clone (contains shared JIT engine), so we clone for the factory
        let compiled_for_pool = compiled.clone();
        let pool = PoolBuilder::new()
            .with_max_instances(std::thread::available_parallelism().unwrap().into())
            .build(move || {
                Plugin::new_from_compiled(&compiled_for_pool)
                    .map_err(|e| anyhow::anyhow!("failed to create lambda instance: {}", e))
            });

        self.compiled.insert(name.to_string(), compiled);
        self.pools.insert(name.to_string(), pool);

        tracing::info!(
            "loaded lambda '{}' from {} (network: {:?}, fs: {:?})",
            name,
            wasm_path.display(),
            capabilities.network.enabled,
            capabilities.fs.enabled
        );

        // Store lambda context (merged manifest + config)
        let merged_manifest = LambdaManifest {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            capabilities: capabilities.clone(),
            features: manifest.features.clone(),
            settings: manifest.settings.clone(),
        };
        let ctx = LambdaContext::new(merged_manifest, config.clone(), self.use_system_proxy);
        self.contexts.insert(name.to_string(), ctx);

        Ok(())
    }

    /// Call a lambda function with typed input/output, handling MessagePack serialization automatically.
    ///
    /// This is the preferred method for calling lambda functions - it handles
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
            .ok_or_else(|| Error::Lambda(format!("lambda '{}' not found", name)))?;

        let timeout = Duration::from_secs(120);
        let mut lambda = pool
            .get(timeout)
            .map_err(|e| Error::Lambda(format!("pool get timeout: {}", e)))?
            .ok_or_else(|| Error::Lambda("pool get timeout".into()))?;

        lambda
            .call(function, input)
            .map_err(|e| Error::Lambda(format!("call failed: {}", e)))
    }

    /// Call a tool function on a lambda.
    ///
    /// This is a convenience method specifically for tool execution that constructs
    /// the ToolExecutionRequest internally and calls the lambda's `execute_tool` function.
    pub fn call_tool(
        &self,
        lambda_name: &str,
        tool_name: &str,
        arguments: &HashMap<String, serde_json::Value>,
    ) -> Result<ToolExecutionResponse, Error> {
        let request = ToolExecutionRequest {
            name: tool_name.to_string(),
            arguments: arguments.clone(),
        };
        self.call(lambda_name, "execute_tool", &request)
    }

    pub fn has_lambda(&self, name: &str) -> bool {
        self.pools.contains_key(name)
    }

    pub fn lambda_count(&self) -> usize {
        self.pools.len()
    }

    /// Get a list of all loaded lambda names
    pub fn lambda_names(&self) -> Vec<String> {
        self.pools.keys().cloned().collect()
    }

    /// Get a list of lambda names that declared features.tool = true
    pub fn tool_lambdas(&self) -> Vec<String> {
        self.contexts
            .iter()
            .filter(|(_, ctx)| ctx.manifest.features.tool)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get the lambda context (manifest + config) for a loaded lambda
    pub fn lambda_context(&self, name: &str) -> Option<&LambdaContext> {
        self.contexts.get(name)
    }

    /// Load a discovered lambda with default config
    pub fn load_discovered(&mut self, lambda: DiscoveredLambda) -> Result<(), Error> {
        self.load_lambda(
            &lambda.manifest.name,
            &lambda.wasm_path,
            &lambda.manifest,
            &LambdaConfig::default(),
        )
    }

    /// Get a reference to the KV store
    pub fn kv(&self) -> Arc<LambdaKV> {
        self.kv.clone()
    }
}

impl Default for LambdaHost {
    fn default() -> Self {
        Self::new()
    }
}
