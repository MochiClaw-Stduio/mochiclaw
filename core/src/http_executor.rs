//! Async HTTP Executor - 执行插件返回的 HTTP Effect
//!
//! 插件通过 lambda_function 返回 HttpEffect，主机负责异步执行这些 HTTP 请求
//! 执行时会应用插件的权限配置（allowed_hosts、denied_hosts）和代理设置

use crate::error::Error;
use mochiclaw_lambda::PluginHost;
use mochiclaw_sdk::lambda::{Effect, EffectResult, HttpEffect};
use std::sync::Arc;

/// 异步 HTTP 执行器
#[derive(Clone)]
pub struct AsyncHttpExecutor {
    plugin_host: Arc<PluginHost>,
}

impl AsyncHttpExecutor {
    /// 创建新的 HTTP 执行器
    pub fn new(plugin_host: Arc<PluginHost>) -> Self {
        Self { plugin_host }
    }

    /// Build a reqwest client with optional proxy
    fn build_client(
        proxy_url: Option<&str>,
        use_system_proxy: bool,
    ) -> Result<reqwest::Client, Error> {
        let client = if let Some(proxy) = proxy_url {
            let proxy = reqwest::Proxy::https(proxy)
                .or_else(|_| reqwest::Proxy::http(proxy))
                .map_err(|e| Error::Http(format!("invalid proxy URL: {}", e)))?;
            reqwest::Client::builder()
                .proxy(proxy)
                .build()
                .map_err(|e| Error::Http(format!("failed to build client with proxy: {}", e)))?
        } else if use_system_proxy {
            reqwest::Client::new()
        } else {
            reqwest::Client::builder()
                .no_proxy()
                .build()
                .map_err(|e| Error::Http(format!("failed to build client: {}", e)))?
        };
        Ok(client)
    }

    /// 执行一个 HTTP Effect，返回响应体字符串
    pub async fn execute(&self, plugin_name: &str, effect: HttpEffect) -> Result<String, Error> {
        // Get plugin context for permissions and proxy
        let ctx = self
            .plugin_host
            .plugin_context(plugin_name)
            .ok_or_else(|| Error::Plugin(format!("plugin '{}' not found", plugin_name)))?;

        // Check if network is enabled
        if !ctx.network_enabled() {
            return Err(Error::Http(format!(
                "network access is disabled for plugin '{}'",
                plugin_name
            )));
        }

        // Check host permissions
        if !ctx.is_host_allowed(&effect.url) {
            return Err(Error::Http(format!(
                "host not allowed for plugin '{}': {}",
                plugin_name, effect.url
            )));
        }

        // Build client with proxy if configured
        let client = Self::build_client(ctx.proxy_url(), ctx.use_system_proxy())?;

        let mut req = match effect.method.to_uppercase().as_str() {
            "GET" => client.get(&effect.url),
            "POST" => client.post(&effect.url),
            "PUT" => client.put(&effect.url),
            "DELETE" => client.delete(&effect.url),
            "PATCH" => client.patch(&effect.url),
            "HEAD" => client.head(&effect.url),
            _ => return Err(Error::Http(format!("unknown method: {}", effect.method))),
        };

        for (k, v) in &effect.headers {
            req = req.header(k, v);
        }

        if let Some(body) = effect.body {
            req = req.body(body);
        }

        let resp = req
            .timeout(std::time::Duration::from_millis(effect.timeout_ms as u64))
            .send()
            .await
            .map_err(|e| Error::Http(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Http(format!("HTTP {}: {}", status, body)));
        }

        let body = resp.text().await.map_err(|e| Error::Http(e.to_string()))?;
        Ok(body)
    }

    /// 执行一个 Effect 枚举（如果是否 HTTP 类型则返回错误）
    pub async fn execute_effect(
        &self,
        plugin_name: &str,
        effect: &Effect,
    ) -> Result<String, Error> {
        match effect {
            Effect::HttpRequest(http_effect) => {
                self.execute(plugin_name, http_effect.clone()).await
            }
        }
    }

    /// 并行执行多个 Effect，返回每个 Effect 的执行结果
    pub async fn execute_all(&self, plugin_name: &str, effects: Vec<Effect>) -> Vec<EffectResult> {
        // 并行执行所有 effects
        let futures: Vec<_> = effects
            .into_iter()
            .map(|effect| async {
                match effect {
                    Effect::HttpRequest(http_effect) => {
                        match self.execute(plugin_name, http_effect).await {
                            Ok(response) => EffectResult {
                                success: true,
                                response: Some(response),
                                error: None,
                            },
                            Err(e) => EffectResult {
                                success: false,
                                response: None,
                                error: Some(e.to_string()),
                            },
                        }
                    }
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }
}
