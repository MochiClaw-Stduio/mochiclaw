//! Lambda runtime context - stores per-lambda manifest and config
//!
//! This module provides LambdaContext which holds the merged manifest
//! and config for each loaded lambda.

use crate::manifest::LambdaManifest;
use mochiclaw_config::LambdaConfig;

/// Complete runtime context for a lambda
#[derive(Debug, Clone)]
pub struct LambdaContext {
    /// Lambda manifest (with capabilities merged from config overrides)
    pub manifest: LambdaManifest,
    /// Lambda configuration
    pub config: LambdaConfig,
    /// Whether to use system proxy when proxy_url is not set
    pub use_system_proxy: bool,
}

impl LambdaContext {
    /// Create a new LambdaContext
    pub fn new(manifest: LambdaManifest, config: LambdaConfig, use_system_proxy: bool) -> Self {
        Self {
            manifest,
            config,
            use_system_proxy,
        }
    }

    /// Get effective proxy URL (config.proxy_url or None if not set)
    pub fn proxy_url(&self) -> Option<&str> {
        self.config.proxy_url.as_deref()
    }

    /// Get use_system_proxy setting
    pub fn use_system_proxy(&self) -> bool {
        self.use_system_proxy
    }

    /// Check if network is enabled
    pub fn network_enabled(&self) -> bool {
        self.manifest.capabilities.network.enabled
    }

    /// Get allowed hosts
    pub fn allowed_hosts(&self) -> &[String] {
        &self.manifest.capabilities.network.allowed_hosts
    }

    /// Get denied hosts
    pub fn denied_hosts(&self) -> &[String] {
        &self.manifest.capabilities.network.denied_hosts
    }

    /// Check if a host is allowed to be accessed
    /// Blacklist takes precedence over whitelist
    pub fn is_host_allowed(&self, url_str: &str) -> bool {
        if !self.network_enabled() {
            return false;
        }

        let allowed = self.allowed_hosts();
        if allowed.is_empty() {
            return false;
        }

        let Ok(url) = url::Url::parse(url_str) else {
            return false;
        };

        let host_str = url.host_str().unwrap_or_default();

        // First check blacklist (denied_hosts takes precedence)
        if self.denied_hosts().iter().any(|pattern| {
            if let Ok(pat) = glob::Pattern::new(pattern) {
                pat.matches(host_str)
            } else {
                pattern == host_str
            }
        }) {
            tracing::warn!(
                "HTTP request to {} is denied by denied_hosts pattern",
                url_str
            );
            return false;
        }

        // Then check whitelist
        allowed.iter().any(|pattern| {
            if let Ok(pat) = glob::Pattern::new(pattern) {
                pat.matches(host_str)
            } else {
                pattern == host_str
            }
        })
    }
}

/// Collection of all lambda contexts
pub type LambdaContextMap = std::collections::HashMap<String, LambdaContext>;
