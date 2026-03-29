//! Test plugin for KV integration testing
//!
//! This plugin exposes functions to test host KV operations.

use mochiclaw_sdk::host::kv::{kv_get, kv_list_readable, kv_list_writable, kv_remove, kv_set};
use mochiclaw_sdk::{FnResult, plugin_fn};
use serde::{Deserialize, Serialize};

/// Test result structure
#[derive(Serialize, Deserialize)]
struct TestResult {
    success: bool,
    message: String,
}

/// Test kv_set operation
#[plugin_fn]
pub fn test_kv_set(_params: String) -> FnResult<String> {
    let key = "test_key";
    let value = "test_value_from_plugin";

    let success = kv_set(key, &value);
    let result = TestResult {
        success,
        message: if success {
            "set ok".to_string()
        } else {
            "set failed".to_string()
        },
    };
    Ok(serde_json::to_string(&result).unwrap_or_default())
}

/// Test kv_get operation
#[plugin_fn]
pub fn test_kv_get(_params: String) -> FnResult<String> {
    let key = "test_key";

    match kv_get::<String>(key) {
        Some(value) => {
            let result = TestResult {
                success: true,
                message: value,
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        None => {
            let result = TestResult {
                success: false,
                message: "key not found".to_string(),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test kv_remove operation
#[plugin_fn]
pub fn test_kv_remove(_params: String) -> FnResult<String> {
    let key = "test_key";

    let success = kv_remove(key);
    let result = TestResult {
        success,
        message: if success {
            "remove ok".to_string()
        } else {
            "remove failed or not found".to_string()
        },
    };
    Ok(serde_json::to_string(&result).unwrap_or_default())
}

/// Test kv_list_readable operation
#[plugin_fn]
pub fn test_kv_list_readable(_params: String) -> FnResult<String> {
    match kv_list_readable() {
        Some(plugins) => Ok(serde_json::to_string(&plugins).unwrap_or_default()),
        None => Ok("[]".to_string()),
    }
}

/// Test kv_list_writable operation
#[plugin_fn]
pub fn test_kv_list_writable(_params: String) -> FnResult<String> {
    match kv_list_writable() {
        Some(plugins) => Ok(serde_json::to_string(&plugins).unwrap_or_default()),
        None => Ok("[]".to_string()),
    }
}

/// Get plugin name
#[plugin_fn]
pub fn get_name(_params: String) -> FnResult<String> {
    Ok("test-kv-plugin".to_string())
}
