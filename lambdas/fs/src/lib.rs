//! Filesystem Tool Lambda
//!
//! Provides read_file, write_file, edit_file, and list_dir tools.
//! Uses lambda_function as the unified entry point.

use std::collections::HashMap;

use mochiclaw_sdk::host::fs;
use mochiclaw_sdk::lambda::{Action, ExecuteToolInput, LambdaInput, LambdaOutput};
use mochiclaw_sdk::tool::{Tool, ToolExecutionResponse};
use mochiclaw_sdk::{FnResult, ToBytes, config, plugin_fn};

// ============================================================================
// Lambda Function Entry Point
// ============================================================================

/// Unified lambda entry point for tool operations
#[plugin_fn]
pub fn lambda_function(params: LambdaInput) -> FnResult<LambdaOutput> {
    match params.action {
        Action::GetTools => handle_get_tools(),
        Action::ExecuteTool => handle_execute_tool(params),
        _ => Ok(LambdaOutput {
            effects: vec![],
            result: ToolExecutionResponse {
                result: String::new(),
                error: Some("fs lambda only supports GetTools and ExecuteTool".to_string()),
            }
            .to_bytes()?,
            new_state: Vec::new(),
        }),
    }
}

// ============================================================================
// Tool Handlers
// ============================================================================

fn handle_get_tools() -> FnResult<LambdaOutput> {
    let tools = vec![
        make_read_file_tool(),
        make_write_file_tool(),
        make_edit_file_tool(),
        make_list_dir_tool(),
    ];
    let tools_json = serde_json::to_string(&tools).unwrap();
    Ok(LambdaOutput {
        effects: vec![],
        result: rmp_serde::to_vec(&tools_json)?,
        new_state: Vec::new(),
    })
}

fn handle_execute_tool(params: LambdaInput) -> FnResult<LambdaOutput> {
    let input: ExecuteToolInput = rmp_serde::from_slice(&params.payload)?;

    // Get workspace from config (injected by host)
    let workspace = match config::get("workspace") {
        Ok(Some(w)) => w,
        Ok(None) => ".".to_string(),
        Err(_) => ".".to_string(),
    };

    let result = match input.name.as_str() {
        "read_file" => {
            let path = match get_string_arg(&input.arguments, "path") {
                Ok(p) => p,
                Err(e) => return error_output(e),
            };
            let offset = input
                .arguments
                .get("offset")
                .and_then(|v: &serde_json::Value| v.as_u64())
                .unwrap_or(1);
            let limit = input
                .arguments
                .get("limit")
                .and_then(|v: &serde_json::Value| v.as_u64())
                .unwrap_or(2000);
            fs::fs_read(&path, &workspace, offset, limit)
        }
        "write_file" => {
            let path = match get_string_arg(&input.arguments, "path") {
                Ok(p) => p,
                Err(e) => return error_output(e),
            };
            let content = match get_string_arg(&input.arguments, "content") {
                Ok(c) => c,
                Err(e) => return error_output(e),
            };
            match fs::fs_write(&path, &workspace, &content) {
                Ok(true) => Ok("File written successfully".to_string()),
                Ok(false) => Err("Write failed".to_string()),
                Err(e) => Err(e),
            }
        }
        "edit_file" => {
            let path = match get_string_arg(&input.arguments, "path") {
                Ok(p) => p,
                Err(e) => return error_output(e),
            };
            let old_text = match get_string_arg(&input.arguments, "old_text") {
                Ok(t) => t,
                Err(e) => return error_output(e),
            };
            let new_text = match get_string_arg(&input.arguments, "new_text") {
                Ok(t) => t,
                Err(e) => return error_output(e),
            };
            let replace_all = input
                .arguments
                .get("replace_all")
                .and_then(|v: &serde_json::Value| v.as_bool())
                .unwrap_or(false);
            fs::fs_edit(&path, &workspace, &old_text, &new_text, replace_all)
        }
        "list_dir" => {
            let path = match get_string_arg(&input.arguments, "path") {
                Ok(p) => p,
                Err(e) => return error_output(e),
            };
            let recursive = input
                .arguments
                .get("recursive")
                .and_then(|v: &serde_json::Value| v.as_bool())
                .unwrap_or(false);
            let max_entries = input
                .arguments
                .get("max_entries")
                .and_then(|v: &serde_json::Value| v.as_u64())
                .unwrap_or(200);
            fs::fs_list(&path, &workspace, recursive, max_entries)
        }
        _ => Err(format!(
            "Unknown tool: '{}'. Available tools: read_file, write_file, edit_file, list_dir",
            input.name
        )),
    };

    match result {
        Ok(result_str) => {
            let response = ToolExecutionResponse {
                result: result_str,
                error: None,
            };
            Ok(LambdaOutput {
                effects: vec![],
                result: response.to_bytes()?,
                new_state: Vec::new(),
            })
        }
        Err(err_str) => error_output(err_str),
    }
}

fn error_output(message: String) -> FnResult<LambdaOutput> {
    let response = ToolExecutionResponse {
        result: String::new(),
        error: Some(message),
    };
    Ok(LambdaOutput {
        effects: vec![],
        result: response.to_bytes()?,
        new_state: Vec::new(),
    })
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

fn get_string_arg(
    arguments: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Missing or invalid argument: {}", key))
}
