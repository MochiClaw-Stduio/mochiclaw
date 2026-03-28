//! HTTP host functions implementation using reqwest with proxy support
//!
//! This module overrides extism's built-in HTTP functions to provide:
//! - Proxy support via reqwest
//! - Better error handling

use extism::{CurrentPlugin, Function, UserData, Val, ValType};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// HTTP request structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpRequest {
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub method: Option<String>,
}

/// Shared HTTP state - wrapped in Arc<Mutex<>> and shared across all HTTP host functions
/// This is the same pattern as extism's CurrentPlugin having http_status/http_headers fields,
/// but we use explicit Arc<Mutex<>> since we can't modify CurrentPlugin.
pub struct HttpContext {
    pub proxy_url: Option<String>,
    pub client: Client,
    pub allowed_hosts: Vec<String>,
    pub denied_hosts: Vec<String>,
    pub last_status: u16,
    pub last_headers: HashMap<String, String>,
}

impl Clone for HttpContext {
    fn clone(&self) -> Self {
        Self {
            proxy_url: self.proxy_url.clone(),
            client: self.client.clone(),
            allowed_hosts: self.allowed_hosts.clone(),
            denied_hosts: self.denied_hosts.clone(),
            last_status: self.last_status,
            last_headers: self.last_headers.clone(),
        }
    }
}

impl HttpContext {
    /// Create a new HTTP context with optional proxy URL and allowed/denied hosts
    ///
    /// - If `proxy_url` is Some, use it directly
    /// - If `proxy_url` is None and `use_system_proxy` is true, reqwest uses system HTTP_PROXY
    /// - If `proxy_url` is None and `use_system_proxy` is false, no proxy is used
    pub fn new(
        proxy_url: Option<String>,
        allowed_hosts: Vec<String>,
        denied_hosts: Vec<String>,
        use_system_proxy: bool,
    ) -> anyhow::Result<Self> {
        let client = if let Some(ref proxy) = proxy_url {
            let proxy = reqwest::Proxy::https(proxy).or_else(|_| reqwest::Proxy::http(proxy))?;
            Client::builder().proxy(proxy).build()?
        } else if use_system_proxy {
            // No explicit proxy, but system proxy is allowed
            Client::new()
        } else {
            // No proxy at all - explicitly disable system proxy
            Client::builder().no_proxy().build()?
        };

        Ok(Self {
            proxy_url,
            client,
            allowed_hosts,
            denied_hosts,
            last_status: 0,
            last_headers: HashMap::new(),
        })
    }

    /// Check if a host is allowed to be accessed
    /// Blacklist takes precedence over whitelist
    fn is_host_allowed(&self, url_str: &str) -> bool {
        if self.allowed_hosts.is_empty() {
            return false;
        }

        let Ok(url) = url::Url::parse(url_str) else {
            return false;
        };

        let host_str = url.host_str().unwrap_or_default();

        // First check blacklist (denied_hosts takes precedence)
        if self.denied_hosts.iter().any(|pattern| {
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
        self.allowed_hosts.iter().any(|pattern| {
            if let Ok(pat) = glob::Pattern::new(pattern) {
                pat.matches(host_str)
            } else {
                pattern == host_str
            }
        })
    }
}

/// Create all HTTP host functions with the given context
pub fn http_functions(ctx: HttpContext) -> Vec<Function> {
    // Wrap in Arc<Mutex<>> ONCE so all three functions share the same state
    // (like extism's CurrentPlugin has http_status/http_headers fields)
    let shared: Arc<Mutex<HttpContext>> = Arc::new(Mutex::new(ctx));

    vec![
        http_request_fn(shared.clone()).with_namespace("extism:host/env"),
        http_status_code_fn(shared.clone()).with_namespace("extism:host/env"),
        http_headers_fn(shared).with_namespace("extism:host/env"),
    ]
}

/// host_http_request: make an HTTP request
///
/// Input: JSON encoded HttpRequest (i64 offset)
/// Body: bytes (i64 offset or 0)
/// Returns: i64 (offset to response body)
fn http_request_fn(ctx: Arc<Mutex<HttpContext>>) -> Function {
    // We pass the Arc directly to the closure, NOT through UserData
    // UserData would wrap it in another Arc, breaking shared state
    Function::new(
        "http_request",
        [ValType::I64, ValType::I64],
        [ValType::I64],
        UserData::new(()),
        move |plugin: &mut CurrentPlugin, inputs: &[Val], outputs: &mut [Val], _user_data: UserData<()>| {
            let mut ctx = match ctx.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            // Parse HTTP request from memory
            let req_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            let body_offset = inputs.get(1).and_then(|v| v.i64()).unwrap_or(0) as u64;

            let req: HttpRequest = match plugin.memory_handle(req_offset) {
                Some(h) => {
                    let bytes = match plugin.memory_bytes(h) {
                        Ok(b) => b.to_vec(),
                        Err(_) => {
                            outputs[0] = Val::I64(0);
                            return Ok(());
                        }
                    };
                    match serde_json::from_slice(&bytes) {
                        Ok(r) => r,
                        Err(_) => {
                            outputs[0] = Val::I64(0);
                            return Ok(());
                        }
                    }
                }
                None => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            // Get request body if provided
            let body: Option<Vec<u8>> = if body_offset > 0 {
                match plugin.memory_handle(body_offset) {
                    Some(h) => plugin.memory_bytes(h).ok().map(|b| b.to_vec()),
                    None => None,
                }
            } else {
                None
            };

            // Check allowed_hosts
            if !ctx.is_host_allowed(&req.url) {
                tracing::warn!(
                    "HTTP request to {} is not allowed (allowed_hosts: {:?})",
                    req.url,
                    ctx.allowed_hosts
                );
                ctx.last_status = 0;
                ctx.last_headers.clear();
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            // Build and execute request
            let method = req.method.as_deref().unwrap_or("GET");
            let mut request = ctx
                .client
                .request(method.parse().unwrap_or(reqwest::Method::GET), &req.url);

            for (k, v) in &req.headers {
                request = request.header(k, v);
            }

            if let Some(body) = body {
                request = request.body(body);
            }

            // Execute request (blocking)
            let response = match request.send() {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::error!("HTTP request failed: {}", e);
                    ctx.last_status = 0;
                    ctx.last_headers.clear();
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            // Store status and headers
            ctx.last_status = response.status().as_u16();

            let headers: HashMap<String, String> = response
                .headers()
                .iter()
                .filter_map(
                    |(k, v): (&reqwest::header::HeaderName, &reqwest::header::HeaderValue)| {
                        v.to_str().ok().map(|v| (k.to_string(), v.to_string()))
                    },
                )
                .collect();

            ctx.last_headers = headers;

            // Read response body
            let body = match response.bytes() {
                Ok(b) => b.to_vec(),
                Err(e) => {
                    tracing::error!("Failed to read response body: {}", e);
                    Vec::new()
                }
            };

            // Allocate memory in plugin and return offset
            match plugin.memory_set_val(&mut outputs[0], &body) {
                Ok(_) => {}
                Err(_) => outputs[0] = Val::I64(0),
            }

            Ok(())
        },
    )
}

/// host_http_status_code: get the status code of the last HTTP request
///
/// Returns: i32 (status code)
fn http_status_code_fn(ctx: Arc<Mutex<HttpContext>>) -> Function {
    Function::new(
        "http_status_code",
        [],
        [ValType::I32],
        UserData::new(()),
        move |_: &mut CurrentPlugin, _: &[Val], outputs: &mut [Val], _user_data: UserData<()>| {
            let ctx = match ctx.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    outputs[0] = Val::I32(0);
                    return Ok(());
                }
            };

            outputs[0] = Val::I32(ctx.last_status as i32);
            Ok(())
        },
    )
}

/// host_http_headers: get the headers of the last HTTP request
///
/// Returns: i64 (offset to JSON encoded headers)
fn http_headers_fn(ctx: Arc<Mutex<HttpContext>>) -> Function {
    Function::new(
        "http_headers",
        [],
        [ValType::I64],
        UserData::new(()),
        move |plugin: &mut CurrentPlugin, _: &[Val], outputs: &mut [Val], _user_data: UserData<()>| {
            let ctx = match ctx.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            let json = match serde_json::to_string(&ctx.last_headers) {
                Ok(s) => s,
                Err(_) => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            match plugin.memory_set_val(&mut outputs[0], &json) {
                Ok(_) => {}
                Err(_) => outputs[0] = Val::I64(0),
            }

            Ok(())
        },
    )
}