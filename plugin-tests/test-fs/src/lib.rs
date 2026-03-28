//! Test plugin for filesystem integration testing
//!
//! This plugin exposes functions to test host filesystem operations.

use mochiclaw_sdk::host::fs::{fs_edit, fs_list, fs_read, fs_write};
use mochiclaw_sdk::{FnResult, plugin_fn};
use serde::{Deserialize, Serialize};

/// Test result structure
#[derive(Serialize, Deserialize)]
struct TestResult {
    success: bool,
    message: String,
}

/// Test fs_write and fs_read operations
#[plugin_fn]
pub fn test_fs_write_read(_params: String) -> FnResult<String> {
    let test_file = "test_file.txt";
    let workspace = "";
    let content = "Hello, filesystem test!\nLine 2\nLine 3";

    // Write the file
    let write_result = fs_write(test_file, workspace, content);
    if write_result.is_err() {
        let result = TestResult {
            success: false,
            message: format!("write error: {}", write_result.unwrap_err()),
        };
        return Ok(serde_json::to_string(&result).unwrap_or_default());
    }

    // Read the file back
    let read_result = fs_read(test_file, workspace, 1, 100);
    match read_result {
        Ok(content) => {
            let result = TestResult {
                success: content.contains("Hello"),
                message: "read ok".to_string(),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("read error: {}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test fs_edit operation
#[plugin_fn]
pub fn test_fs_edit(_params: String) -> FnResult<String> {
    let test_file = "test_edit.txt";
    let workspace = "";
    let original = "Hello World";
    let _edited = "Hello Rust";

    // Write original content
    let write_result = fs_write(test_file, workspace, original);
    if write_result.is_err() {
        let result = TestResult {
            success: false,
            message: format!("write error: {}", write_result.unwrap_err()),
        };
        return Ok(serde_json::to_string(&result).unwrap_or_default());
    }

    // Edit the file
    let edit_result = fs_edit(test_file, workspace, "World", "Rust", false);
    match edit_result {
        Ok(msg) => {
            let result = TestResult {
                success: msg.contains("Successfully"),
                message: msg,
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("edit error: {}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test fs_edit with replace_all
#[plugin_fn]
pub fn test_fs_edit_all(_params: String) -> FnResult<String> {
    let test_file = "test_edit_all.txt";
    let workspace = "";
    let original = "foo bar foo baz foo";

    // Write original content
    let write_result = fs_write(test_file, workspace, original);
    if write_result.is_err() {
        let result = TestResult {
            success: false,
            message: format!("write error: {}", write_result.unwrap_err()),
        };
        return Ok(serde_json::to_string(&result).unwrap_or_default());
    }

    // Replace all "foo" with "qux"
    let edit_result = fs_edit(test_file, workspace, "foo", "qux", true);
    match edit_result {
        Ok(_msg) => {
            // Verify all replacements were made
            let read_result = fs_read(test_file, workspace, 1, 100);
            match read_result {
                Ok(content) => {
                    let success = content.contains("qux bar qux baz qux");
                    let result = TestResult {
                        success,
                        message: if success {
                            "replace_all ok".to_string()
                        } else {
                            content
                        },
                    };
                    Ok(serde_json::to_string(&result).unwrap_or_default())
                }
                Err(e) => {
                    let result = TestResult {
                        success: false,
                        message: format!("read error: {}", e),
                    };
                    Ok(serde_json::to_string(&result).unwrap_or_default())
                }
            }
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("edit error: {}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test fs_list operation
#[plugin_fn]
pub fn test_fs_list(_params: String) -> FnResult<String> {
    // List the root workspace directory (non-recursive)
    let result = fs_list(".", "", false, 50);

    match result {
        Ok(listing) => {
            let result = TestResult {
                success: true,
                message: listing.chars().take(100).collect(),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("list error: {}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test fs_list with recursive
#[plugin_fn]
pub fn test_fs_list_recursive(_params: String) -> FnResult<String> {
    // List recursively with a limit
    let result = fs_list(".", "", true, 100);

    match result {
        Ok(listing) => {
            let result = TestResult {
                success: true,
                message: format!("entries shown: {}", listing.lines().count()),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("list recursive error: {}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Test fs_read with offset and limit
#[plugin_fn]
pub fn test_fs_read_pagination(_params: String) -> FnResult<String> {
    let test_file = "test_pagination.txt";
    let workspace = "";
    let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";

    // Write a file with multiple lines
    let write_result = fs_write(test_file, workspace, content);
    if write_result.is_err() {
        let result = TestResult {
            success: false,
            message: format!("write error: {}", write_result.unwrap_err()),
        };
        return Ok(serde_json::to_string(&result).unwrap_or_default());
    }

    // Read with offset=2, limit=2 (should get lines 2-3)
    let read_result = fs_read(test_file, workspace, 2, 2);
    match read_result {
        Ok(content) => {
            let result = TestResult {
                success: content.contains("Line 2") && content.contains("Line 3"),
                message: "pagination ok".to_string(),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
        Err(e) => {
            let result = TestResult {
                success: false,
                message: format!("read error: {}", e),
            };
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }
    }
}

/// Get plugin name
#[plugin_fn]
pub fn get_name(_params: String) -> FnResult<String> {
    Ok("test-fs-plugin".to_string())
}
