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

/// HTTP request structure (same as extism_manifest::HttpRequest)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpRequest {
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub method: Option<String>,
}

/// HTTP context for storing proxy config and last response state
///
/// Wrapped in Arc<Mutex<>> to share state across all HTTP host functions
/// (http_request, http_status_code, http_headers)
#[derive(Clone)]
pub struct HttpContext {
    inner: Arc<Mutex<HttpContextInner>>,
}

pub struct HttpContextInner {
    pub proxy_url: Option<String>,
    pub client: Client,
    pub allowed_hosts: Vec<String>,
    pub last_status: u16,
    pub last_headers: HashMap<String, String>,
}

impl HttpContext {
    /// Create a new HTTP context with optional proxy URL and allowed hosts
    pub fn new(proxy_url: Option<String>, allowed_hosts: Vec<String>) -> anyhow::Result<Self> {
        let client = if let Some(ref proxy) = proxy_url {
            let proxy = reqwest::Proxy::https(proxy).or_else(|_| reqwest::Proxy::http(proxy))?;
            Client::builder().proxy(proxy).build()?
        } else {
            Client::new()
        };

        Ok(Self {
            inner: Arc::new(Mutex::new(HttpContextInner {
                proxy_url,
                client,
                allowed_hosts,
                last_status: 0,
                last_headers: HashMap::new(),
            })),
        })
    }

    /// Create a new HTTP context without proxy
    pub fn new_without_proxy(allowed_hosts: Vec<String>) -> anyhow::Result<Self> {
        Self::new(None, allowed_hosts)
    }

    /// Check if a host is allowed to be accessed
    fn is_host_allowed(&self, url_str: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        if inner.allowed_hosts.is_empty() {
            return false;
        }

        let url = match url::Url::parse(url_str) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let host_str = url.host_str().unwrap_or_default();

        inner.allowed_hosts.iter().any(|pattern| {
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
    vec![
        http_request_fn(ctx.clone()).with_namespace("extism:host/env"),
        http_status_code_fn(ctx.clone()).with_namespace("extism:host/env"),
        http_headers_fn(ctx).with_namespace("extism:host/env"),
    ]
}

/// host_http_request: make an HTTP request
///
/// Input: JSON encoded HttpRequest (i64 offset)
/// Body: bytes (i64 offset or 0)
/// Returns: i64 (offset to response body)
fn http_request_fn(ctx: HttpContext) -> Function {
    Function::new(
        "http_request",
        [ValType::I64, ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<HttpContext>| {
            // Get context from user_data
            let ctx_arc = match user_data.get() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("http_request failed to get context: {}", e);
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            let mut ctx = match ctx_arc.lock() {
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
                    ctx.inner.lock().unwrap().allowed_hosts
                );
                ctx.inner.lock().unwrap().last_status = 0;
                ctx.inner.lock().unwrap().last_headers.clear();
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            // Build and execute request
            let method = req.method.as_deref().unwrap_or("GET");
            let mut request = ctx
                .inner
                .lock()
                .unwrap()
                .client
                .request(method.parse().unwrap_or(reqwest::Method::GET), &req.url);

            for (k, v) in &req.headers {
                request = request.header(k, v);
            }

            let body = if let Some(body) = body {
                request = request.body(body);
                None
            } else {
                body
            };

            // Execute request (blocking)
            let response = match request.send() {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::error!("HTTP request failed: {}", e);
                    // Set error status
                    ctx.inner.lock().unwrap().last_status = 0;
                    ctx.inner.lock().unwrap().last_headers.clear();
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            // Store status and headers
            let status = response.status().as_u16();
            let headers: HashMap<String, String> = response
                .headers()
                .iter()
                .filter_map(
                    |(k, v): (&reqwest::header::HeaderName, &reqwest::header::HeaderValue)| {
                        v.to_str().ok().map(|v| (k.to_string(), v.to_string()))
                    },
                )
                .collect();

            {
                let mut inner = ctx.inner.lock().unwrap();
                inner.last_status = status;
                inner.last_headers = headers;
            }

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
fn http_status_code_fn(ctx: HttpContext) -> Function {
    Function::new(
        "http_status_code",
        [],
        [ValType::I32],
        UserData::new(ctx),
        |_plugin: &mut CurrentPlugin,
         _inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<HttpContext>| {
            let ctx_arc = match user_data.get() {
                Ok(ctx) => ctx,
                Err(_) => {
                    outputs[0] = Val::I32(0);
                    return Ok(());
                }
            };

            let ctx = match ctx_arc.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    outputs[0] = Val::I32(0);
                    return Ok(());
                }
            };

            outputs[0] = Val::I32(ctx.inner.lock().unwrap().last_status as i32);
            Ok(())
        },
    )
}

/// host_http_headers: get the headers of the last HTTP request
///
/// Returns: i64 (offset to JSON encoded headers)
fn http_headers_fn(ctx: HttpContext) -> Function {
    Function::new(
        "http_headers",
        [],
        [ValType::I64],
        UserData::new(ctx),
        |plugin: &mut CurrentPlugin,
         _inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<HttpContext>| {
            let ctx_arc = match user_data.get() {
                Ok(ctx) => ctx,
                Err(_) => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            let ctx = match ctx_arc.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            let json = {
                let inner = ctx.inner.lock().unwrap();
                match serde_json::to_string(&inner.last_headers) {
                    Ok(s) => s,
                    Err(_) => {
                        outputs[0] = Val::I64(0);
                        return Ok(());
                    }
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