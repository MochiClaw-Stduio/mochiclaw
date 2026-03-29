//! HTTP client for plugins with a ergonomic, reqwest-like API
//!
//! This module provides a higher-level HTTP API on top of the extism host functions.

use extism_pdk::Memory;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTTP response returned by [`HttpClient::send`]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl HttpResponse {
    /// Get response body as a string
    pub fn text(&self) -> Result<&str, HttpError> {
        std::str::from_utf8(&self.body).map_err(|_| HttpError::InvalidUtf8)
    }

    /// Deserialize response body as JSON
    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> Result<T, HttpError> {
        serde_json::from_slice(&self.body).map_err(HttpError::Json)
    }

    /// Get raw response bytes
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }
}

/// Errors that can occur when making HTTP requests
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("HTTP request failed: {0}")]
    Request(String),
    #[error("Status code {0} indicates an error")]
    StatusCode(u16),
    #[error("Response is not valid UTF-8")]
    InvalidUtf8,
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

/// HTTP request structure used internally
#[derive(Serialize, Deserialize)]
struct HttpRequest {
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    method: Option<String>,
}

// Raw FFI declarations - matching extism built-in signatures exactly
#[link(wasm_import_module = "extism:host/env")]
unsafe extern "C" {
    fn http_request(req: u64, body: u64) -> u64;
    fn http_status_code() -> i32;
    fn http_headers() -> u64;
}

/// HTTP client for building and sending requests
pub struct HttpClient {
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
}

impl HttpClient {
    /// Create a new HTTP client (defaults to GET request)
    pub fn new() -> Self {
        Self {
            url: String::new(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        }
    }

    /// Set the URL
    pub fn url(mut self, url: &str) -> Self {
        self.url = url.to_string();
        self
    }

    /// Set the HTTP method
    pub fn method(mut self, method: &str) -> Self {
        self.method = method.to_string();
        self
    }

    /// Add a header
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }

    /// Set JSON body and automatically set Content-Type header
    pub fn json<T: Serialize>(mut self, value: &T) -> Result<Self, HttpError> {
        let bytes = serde_json::to_vec(value)?;
        self.body = Some(bytes);
        self.headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        Ok(self)
    }

    /// Set raw body bytes
    pub fn body(mut self, bytes: Vec<u8>) -> Self {
        self.body = Some(bytes);
        self
    }

    /// Send the HTTP request
    pub fn send(self) -> Result<HttpResponse, HttpError> {
        let request = HttpRequest {
            url: self.url,
            method: Some(self.method),
            headers: self.headers,
        };

        // Serialize request to JSON and allocate memory
        let req_bytes =
            serde_json::to_vec(&request).map_err(|e| HttpError::Request(e.to_string()))?;
        let req_mem =
            Memory::from_bytes(&req_bytes).map_err(|e| HttpError::Request(e.to_string()))?;
        let req_offset = req_mem.offset();

        // Handle body
        let body = self.body.unwrap_or_default();
        let body_offset = if body.is_empty() {
            0
        } else {
            let body_mem =
                Memory::from_bytes(&body).map_err(|e| HttpError::Request(e.to_string()))?;
            body_mem.offset()
        };

        // Call http_request
        let response_offset = unsafe { http_request(req_offset, body_offset) };
        if response_offset == 0 {
            return Err(HttpError::Request("HTTP request failed".to_string()));
        }

        // Get response body
        let response_mem = Memory::find(response_offset)
            .ok_or_else(|| HttpError::Request("Failed to find response memory".to_string()))?;
        let response_body: Vec<u8> = response_mem.to_vec();

        // Get status code
        let status = unsafe { http_status_code() };

        // Get headers
        let headers_offset = unsafe { http_headers() };
        let headers: HashMap<String, String> = if headers_offset == 0 {
            HashMap::new()
        } else {
            Memory::find(headers_offset)
                .and_then(|h| serde_json::from_slice(&h.to_vec()).ok())
                .unwrap_or_default()
        };

        Ok(HttpResponse {
            status: status as u16,
            headers,
            body: response_body,
        })
    }
}

// Convenience methods
impl HttpClient {
    /// Start a GET request
    pub fn get(url: &str) -> Self {
        Self::new().url(url)
    }

    /// Start a POST request
    pub fn post(url: &str) -> Self {
        Self::new().method("POST").url(url)
    }

    /// Start a PUT request
    pub fn put(url: &str) -> Self {
        Self::new().method("PUT").url(url)
    }

    /// Start a DELETE request
    pub fn delete(url: &str) -> Self {
        Self::new().method("DELETE").url(url)
    }

    /// Start a PATCH request
    pub fn patch(url: &str) -> Self {
        Self::new().method("PATCH").url(url)
    }

    /// Start a HEAD request
    pub fn head(url: &str) -> Self {
        Self::new().method("HEAD").url(url)
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}
