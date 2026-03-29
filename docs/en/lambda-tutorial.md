# Lambda Tutorial

[简体中文](../zh/lambda-tutorial.md) | English

---

## Overview

This tutorial walks through creating a new tool lambda from scratch.

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
```

## Step 1: Create Lambda Project

Create a new Rust project in the `lambdas/` directory:

```bash
cargo new --target wasm32-unknown-unknown lambdas/mochi-my-lambda
```

## Step 2: Edit Cargo.toml

```toml
[package]
name = "mochi-my-lambda"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
mochiclaw-sdk = { path = "../../sdk" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
extism-pdk = "1"
```

## Step 3: Create manifest.toml

```toml
name = "mochi-my-lambda"
version = "0.1.0"
description = "My custom tool lambda"

[capabilities.network]
enabled = true
allowed_hosts = ["api.example.com"]

[capabilities.fs]
enabled = true
allowed_root = "${workspace}"
read_whitelist = ["${workspace}"]
write_whitelist = ["${workspace}"]

[features]
tool = true
```

## Step 4: Implement Lambda

```rust
// src/lib.rs
use mochiclaw_sdk::tool::{Tool, ToolExecutionRequest, ToolExecutionResponse};
use mochiclaw_sdk::{FnResult, plugin_fn};
use std::collections::HashMap;

/// Return the list of tools provided by this lambda
#[plugin_fn]
pub fn get_tools() -> FnResult<String> {
    let tools = vec![make_my_tool()];
    Ok(serde_json::to_string(&tools).unwrap())
}

/// Execute a tool by name with the provided arguments
#[plugin_fn]
pub fn execute_tool(request: ToolExecutionRequest) -> FnResult<ToolExecutionResponse> {
    let result = match request.name.as_str() {
        "my_tool" => do_something(&request.arguments)?,
        _ => return Err(format!("Unknown tool: {}", request.name).into()),
    };

    Ok(ToolExecutionResponse {
        result,
        error: None,
    })
}

fn make_my_tool() -> Tool {
    let mut params = HashMap::new();
    params.insert("type".to_string(), serde_json::json!("object"));
    params.insert(
        "properties".to_string(),
        serde_json::json!({
            "input": {
                "type": "string",
                "description": "Input for my tool"
            }
        }),
    );
    params.insert("required".to_string(), serde_json::json!(["input"]));

    Tool {
        name: "my_tool".to_string(),
        description: "Does something useful".to_string(),
        parameters: params,
    }
}

fn do_something(args: &HashMap<String, serde_json::Value>) -> Result<String, String> {
    let input = args
        .get("input")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'input' argument")?;

    // Your logic here
    Ok(format!("Processed: {}", input))
}
```

## Step 5: Build

```bash
cargo build --release --target wasm32-unknown-unknown -p mochi-my-lambda
```

Output: `target/wasm32-unknown-unknown/release/mochi_my_lambda.wasm`

## Step 6: Deploy

Copy the WASM file and `manifest.toml` to your lambda directory:

```bash
cp target/wasm32-unknown-unknown/release/mochi_my_lambda.wasm \
   ./target/lambdas/
cp lambdas/mochi-my-lambda/manifest.toml \
   ./target/lambdas/
```

## Lambda Types

### Tool Lambda

Provides tools to the agent:

```rust
#[plugin_fn]
pub fn get_tools() -> FnResult<String>

#[plugin_fn]
pub fn execute_tool(request: ToolExecutionRequest) -> FnResult<ToolExecutionResponse>
```

### Provider Lambda

Provides LLM access:

```rust
#[plugin_fn]
pub fn chat(request: ChatRequest) -> FnResult<ChatResponse>

#[plugin_fn]
pub fn chat_stream(request: ChatRequest) -> FnResult<ChatResponse>  // streaming
```

### Channel Lambda

Handles messaging:

```rust
#[plugin_fn]
pub fn poll(params: PollParams) -> FnResult<PollResponse>

#[plugin_fn]
pub fn send_text(params: SendTextParams) -> FnResult<SendResponse>

#[plugin_fn]
pub fn set_typing(params: SetTypingParams) -> FnResult<()>
```

## Using Host Functions

Access capabilities from `mochiclaw_sdk::host`:

```rust
use mochiclaw_sdk::host::http::{HttpClient, HttpError};
use mochiclaw_sdk::host::fs::{fs_read, fs_write};
use mochiclaw_sdk::host::kv::{kv_get, kv_set};
use mochiclaw_sdk::host::random::{rand_u32, rand_bytes};

// HTTP request
let resp = HttpClient::get("https://api.example.com/data").send()?;

// File read
let content = fs_read("file.txt", workspace, 0, 100)?;

// KV store
kv_set("key", &my_value)?;
let value: MyType = kv_get("key")?;

// Random
let n = rand_u32();
```

## Workspace Configuration

Lambdas receive config from the host via `config::get()`:

```rust
use mochiclaw_sdk::config;

// Get workspace (injected by host)
let workspace = match config::get("workspace") {
    Ok(Some(w)) => w,
    _ => ".".to_string(),
};
```

## Error Handling

```rust
// Return error in tool execution
Err("Something went wrong".into())

// Or in ToolExecutionResponse
Ok(ToolExecutionResponse {
    result: String::new(),
    error: Some("Error message".to_string()),
})
```
