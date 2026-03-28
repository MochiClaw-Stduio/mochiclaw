//! Test plugin for HTTP integration testing
//!
//! This plugin exposes functions to test host HTTP operations.

use mochiclaw_sdk::{plugin_fn, FnResult};
use mochiclaw_sdk::host::http::HttpClient;
use serde::{Deserialize, Serialize};

/// Test result structure
#[derive(Serialize, Deserialize)]
struct TestResult {
    success: bool,
    message: String,
}

/// Test HTTP GET request
#[plugin_fn]
pub fn test_http_get(_params: String) -> FnResult<String> {
    let response = HttpClient::get("https://httpbin.org/get")
        .header("X-Test", "test-http-get")
        .send();

    match response {
        Ok(resp) => {
            let result = TestResult {
                success: resp.status == 200,
                message: format!("status={}", resp.status),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("error={}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test HTTP POST request
#[plugin_fn]
pub fn test_http_post(_params: String) -> FnResult<String> {
    let client = match HttpClient::post("https://httpbin.org/post")
        .header("X-Test", "test-http-post")
        .json(&serde_json::json!({"test": "data"}))
    {
        Ok(c) => c,
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("json error: {}", e),
            };
            return Ok(serde_json::to_string(&result).unwrap_or_default());
        }
    };

    let response = client.send();

    match response {
        Ok(resp) => {
            let result = TestResult {
                success: resp.status == 200,
                message: format!("status={}", resp.status),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("error={}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test HTTP PUT request
#[plugin_fn]
pub fn test_http_put(_params: String) -> FnResult<String> {
    let client = match HttpClient::put("https://httpbin.org/put")
        .header("X-Test", "test-http-put")
        .json(&serde_json::json!({"test": "data"}))
    {
        Ok(c) => c,
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("json error: {}", e),
            };
            return Ok(serde_json::to_string(&result).unwrap_or_default());
        }
    };

    let response = client.send();

    match response {
        Ok(resp) => {
            let result = TestResult {
                success: resp.status == 200,
                message: format!("status={}", resp.status),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("error={}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test HTTP DELETE request
#[plugin_fn]
pub fn test_http_delete(_params: String) -> FnResult<String> {
    let response = HttpClient::delete("https://httpbin.org/delete")
        .header("X-Test", "test-http-delete")
        .send();

    match response {
        Ok(resp) => {
            let result = TestResult {
                success: resp.status == 200,
                message: format!("status={}", resp.status),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("error={}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test HTTP request with custom headers
#[plugin_fn]
pub fn test_http_headers(_params: String) -> FnResult<String> {
    let response = HttpClient::get("https://httpbin.org/headers")
        .header("X-Custom-Header", "CustomValue")
        .header("X-Another-Header", "AnotherValue")
        .send();

    match response {
        Ok(resp) => {
            // Try to parse the response body to verify headers were sent
            match resp.text() {
                Ok(text) => {
                    let result = TestResult {
                        success: resp.status == 200,
                        message: text.chars().take(100).collect(),
                    };
                    Ok(serde_json::to_string(&result).unwrap_or_default())
                }
                Err(e) => {
                    let result = TestResult {
                        success: false,
                        message: format!("failed to read body: {}", e),
                    };
                    Ok(serde_json::to_string(&result).unwrap_or_default())
                }
            }
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("error={}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test HTTP response status code parsing
#[plugin_fn]
pub fn test_http_status(_params: String) -> FnResult<String> {
    // httpbin.org/status/418 returns a teapot response
    let response = HttpClient::get("https://httpbin.org/status/418")
        .send();

    match response {
        Ok(resp) => {
            let result = TestResult {
                success: resp.status == 418,
                message: format!("status={}", resp.status),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("error={}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Get plugin name
#[plugin_fn]
pub fn get_name(_params: String) -> FnResult<String> {
    Ok("test-http-plugin".to_string())
}