//! Test plugin for random integration testing
//!
//! This plugin exposes functions to test host random operations.

use mochiclaw_sdk::{plugin_fn, FnResult};
use serde::{Deserialize, Serialize};
use mochiclaw_sdk::host::random::{rand_u32, rand_u64, rand_bytes};

/// Test result structure
#[derive(Serialize, Deserialize)]
struct TestResult {
    success: bool,
    message: String,
}

/// Test rand_u32 operation
#[plugin_fn]
pub fn test_rand_u32(_params: String) -> FnResult<String> {
    let value = rand_u32();
    let result = TestResult {
        success: true,
        message: value.to_string(),
    };
    Ok(serde_json::to_string(&result).unwrap_or_default())
}

/// Test rand_u64 operation
#[plugin_fn]
pub fn test_rand_u64(_params: String) -> FnResult<String> {
    let value = rand_u64();
    let result = TestResult {
        success: true,
        message: value.to_string(),
    };
    Ok(serde_json::to_string(&result).unwrap_or_default())
}

/// Test rand_bytes operation
#[plugin_fn]
pub fn test_rand_bytes(_params: String) -> FnResult<String> {
    let mut buf = [0u8; 16];
    rand_bytes(&mut buf);
    let result = TestResult {
        success: true,
        message: format!("{:?}", buf),
    };
    Ok(serde_json::to_string(&result).unwrap_or_default())
}
