//! Filesystem Tool Plugin
//!
//! Provides read_file, write_file, edit_file, and list_dir tools.

use std::collections::HashMap;

use mochiclaw_sdk::host::fs;
use mochiclaw_sdk::tool::{Tool, ToolExecutionRequest, ToolExecutionResponse};
use mochiclaw_sdk::{FnResult, config, plugin_fn};

/// Return the list of tools provided by this plugin
#[plugin_fn]
pub fn get_tools() -> FnResult<String> {
    let tools = vec![
        make_read_file_tool(),
        make_write_file_tool(),
        make_edit_file_tool(),
        make_list_dir_tool(),
    ];
    Ok(serde_json::to_string(&tools).unwrap())
}

/// Execute a tool by name with the provided arguments
#[plugin_fn]
pub fn execute_tool(request: ToolExecutionRequest) -> FnResult<ToolExecutionResponse> {
    // Get workspace from config (injected by host)
    let workspace = match config::get("workspace") {
        Ok(Some(w)) => w,
        Ok(None) => ".".to_string(),
        Err(_) => ".".to_string(),
    };

    let result = match request.name.as_str() {
        "read_file" => {
            let path = match get_string_arg(&request, "path") {
                Ok(p) => p,
                Err(e) => return Ok(error_response(e)),
            };
            let offset = request
                .arguments
                .get("offset")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);
            let limit = request
                .arguments
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(2000);
            fs::fs_read(&path, &workspace, offset, limit)
        }
        "write_file" => {
            let path = match get_string_arg(&request, "path") {
                Ok(p) => p,
                Err(e) => return Ok(error_response(e)),
            };
            let content = match get_string_arg(&request, "content") {
                Ok(c) => c,
                Err(e) => return Ok(error_response(e)),
            };
            match fs::fs_write(&path, &workspace, &content) {
                Ok(true) => Ok("File written successfully".to_string()),
                Ok(false) => Err("Write failed".to_string()),
                Err(e) => Err(e),
            }
        }
        "edit_file" => {
            let path = match get_string_arg(&request, "path") {
                Ok(p) => p,
                Err(e) => return Ok(error_response(e)),
            };
            let old_text = match get_string_arg(&request, "old_text") {
                Ok(t) => t,
                Err(e) => return Ok(error_response(e)),
            };
            let new_text = match get_string_arg(&request, "new_text") {
                Ok(t) => t,
                Err(e) => return Ok(error_response(e)),
            };
            let replace_all = request
                .arguments
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            fs::fs_edit(&path, &workspace, &old_text, &new_text, replace_all)
        }
        "list_dir" => {
            let path = match get_string_arg(&request, "path") {
                Ok(p) => p,
                Err(e) => return Ok(error_response(e)),
            };
            let recursive = request
                .arguments
                .get("recursive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let max_entries = request
                .arguments
                .get("max_entries")
                .and_then(|v| v.as_u64())
                .unwrap_or(200);
            fs::fs_list(&path, &workspace, recursive, max_entries)
        }
        _ => Err(format!(
            "Unknown tool: '{}'. Available tools: read_file, write_file, edit_file, list_dir",
            request.name
        )),
    };

    match result {
        Ok(result_str) => Ok(ToolExecutionResponse {
            result: result_str,
            error: None,
        }),
        Err(err_str) => Ok(error_response(err_str)),
    }
}

fn error_response(message: String) -> ToolExecutionResponse {
    ToolExecutionResponse {
        result: String::new(),
        error: Some(message),
    }
}

// ============================================================================
// Tool definitions
// ============================================================================

fn make_read_file_tool() -> Tool {
    let mut params = HashMap::new();
    params.insert("type".to_string(), serde_json::json!("object"));
    params.insert(
        "properties".to_string(),
        serde_json::json!({
            "path": {
                "type": "string",
                "description": "The file path to read"
            },
            "offset": {
                "type": "integer",
                "description": "Line number to start reading from (1-indexed, default 1)",
                "minimum": 1
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of lines to read (default 2000)",
                "minimum": 1
            }
        }),
    );
    params.insert("required".to_string(), serde_json::json!(["path"]));

    Tool {
        name: "read_file".to_string(),
        description: "Read the contents of a file. Returns numbered lines. Use offset and limit to paginate through large files.".to_string(),
        parameters: params,
    }
}

fn make_write_file_tool() -> Tool {
    let mut params = HashMap::new();
    params.insert("type".to_string(), serde_json::json!("object"));
    params.insert(
        "properties".to_string(),
        serde_json::json!({
            "path": {
                "type": "string",
                "description": "The file path to write to"
            },
            "content": {
                "type": "string",
                "description": "The content to write"
            }
        }),
    );
    params.insert(
        "required".to_string(),
        serde_json::json!(["path", "content"]),
    );

    Tool {
        name: "write_file".to_string(),
        description:
            "Write content to a file at the given path. Creates parent directories if needed."
                .to_string(),
        parameters: params,
    }
}

fn make_edit_file_tool() -> Tool {
    let mut params = HashMap::new();
    params.insert("type".to_string(), serde_json::json!("object"));
    params.insert(
        "properties".to_string(),
        serde_json::json!({
            "path": {
                "type": "string",
                "description": "The file path to edit"
            },
            "old_text": {
                "type": "string",
                "description": "The text to find and replace"
            },
            "new_text": {
                "type": "string",
                "description": "The text to replace with"
            },
            "replace_all": {
                "type": "boolean",
                "description": "Replace all occurrences (default false)"
            }
        }),
    );
    params.insert(
        "required".to_string(),
        serde_json::json!(["path", "old_text", "new_text"]),
    );

    Tool {
        name: "edit_file".to_string(),
        description: "Edit a file by replacing old_text with new_text. Supports minor whitespace/line-ending differences. Set replace_all=true to replace every occurrence.".to_string(),
        parameters: params,
    }
}

fn make_list_dir_tool() -> Tool {
    let mut params = HashMap::new();
    params.insert("type".to_string(), serde_json::json!("object"));
    params.insert(
        "properties".to_string(),
        serde_json::json!({
            "path": {
                "type": "string",
                "description": "The directory path to list"
            },
            "recursive": {
                "type": "boolean",
                "description": "Recursively list all files (default false)"
            },
            "max_entries": {
                "type": "integer",
                "description": "Maximum entries to return (default 200)",
                "minimum": 1
            }
        }),
    );
    params.insert("required".to_string(), serde_json::json!(["path"]));

    Tool {
        name: "list_dir".to_string(),
        description: "List the contents of a directory. Set recursive=true to explore nested structure. Common noise directories (.git, node_modules, __pycache__, etc.) are auto-ignored.".to_string(),
        parameters: params,
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn get_string_arg(request: &ToolExecutionRequest, key: &str) -> Result<String, String> {
    request
        .arguments
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Missing or invalid argument: {}", key))
}
