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

## Step 4: Implement Lambda with Unified Entry Point

All lambdas use a unified `lambda_function` entry point with `Action` dispatch:

```rust
// src/lib.rs
use mochiclaw_sdk::lambda::{Action, ExecuteToolInput, LambdaInput, LambdaOutput};
use mochiclaw_sdk::tool::{Tool, ToolExecutionResponse};
use mochiclaw_sdk::{FnResult, plugin_fn};
use std::collections::HashMap;

/// Unified lambda entry point - handles all actions
#[plugin_fn]
pub fn lambda_function(params: LambdaInput) -> FnResult<LambdaOutput> {
    match params.action {
        Action::GetTools => handle_get_tools(),
        Action::ExecuteTool => handle_execute_tool(params),
        _ => Ok(LambdaOutput {
            effects: vec![],
            result: ToolExecutionResponse {
                result: String::new(),
                error: Some("Unsupported action".to_string()),
            }
            .to_bytes()?,
            new_state: Vec::new(),
        }),
    }
}

fn handle_get_tools() -> FnResult<LambdaOutput> {
    let tools = vec![make_my_tool()];
    let tools_json = serde_json::to_string(&tools).unwrap();
    Ok(LambdaOutput {
        effects: vec![],
        result: rmp_serde::to_vec(&tools_json)?,
        new_state: Vec::new(),
    })
}

fn handle_execute_tool(params: LambdaInput) -> FnResult<LambdaOutput> {
    let input: ExecuteToolInput = rmp_serde::from_slice(&params.payload)?;

    let result = match input.name.as_str() {
        "my_tool" => do_something(&input.arguments)?,
        _ => return Err(format!("Unknown tool: {}", input.name).into()),
    };

    let response = ToolExecutionResponse {
        result,
        error: None,
    };
    Ok(LambdaOutput {
        effects: vec![],
        result: response.to_bytes()?,
        new_state: Vec::new(),
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

## Lambda Types & Actions

All lambdas use `lambda_function` with `Action` dispatch. The supported actions depend on the lambda type:

### Tool Lambda

| Action | Input | Output |
|--------|-------|--------|
| `GetTools` | none | `String` (JSON array of tools) |
| `ExecuteTool` | `ExecuteToolInput` | `ToolExecutionResponse` |

### Provider Lambda

| Action | Input | Output |
|--------|-------|--------|
| `Chat` | `ChatInput` | `ChatResponse` |

### Channel Lambda

| Action | Input | Output |
|--------|-------|--------|
| `PreparePoll` | `PreparePollInput` | `DigestOutput` |
| `FormatSend` | `SendInput` | `SendOutput` |
| `SetTyping` | `SetTypingInput` | `()` |
| `Login` | `LoginInput` | `LoginOutput` |
| `CheckLogin` | `CheckLoginInput` | `CheckLoginOutput` |

## Effect System (HTTP)

**Important**: HTTP is no longer a direct host function. Instead, lambdas return `HttpEffect` declarations that the host executes.

For provider lambdas like OpenAI, the `Chat` action typically involves two calls:

1. **First call**: Return `HttpEffect` for the API request
2. **Second call**: With `effect_results` containing the HTTP response, parse and return `ChatResponse`

```rust
// Example: OpenAI provider returns HttpEffect
fn handle_chat(params: LambdaInput) -> FnResult<LambdaOutput> {
    if !params.effect_results.is_empty() {
        // Second call: parse HTTP response
        let response = &params.effect_results[0];
        let chat_response = parse_openai_response(response)?;
        return Ok(LambdaOutput {
            effects: vec![],
            result: chat_response.to_bytes()?,
            new_state: Vec::new(),
        });
    }

    // First call: return HTTP effect
    let effect = HttpEffect {
        method: "POST".to_string(),
        url: "https://api.openai.com/v1/chat/completions".to_string(),
        headers: headers!["Authorization" => format!("Bearer {}", api_key)],
        body: Some(request_body),
        timeout_ms: 60000,
    };
    Ok(LambdaOutput {
        effects: vec![Effect::HttpRequest(effect)],
        result: Vec::new(),
        new_state: Vec::new(),
    })
}
```

## Using Host Functions

Access capabilities from `mochiclaw_sdk::host`:

```rust
use mochiclaw_sdk::host::fs::{fs_read, fs_write};
use mochiclaw_sdk::host::kv::{kv_get, kv_set};
use mochiclaw_sdk::host::random::{rand_u32, rand_bytes};

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

// Or in LambdaOutput with error field
Ok(LambdaOutput {
    effects: vec![],
    result: ToolExecutionResponse {
        result: String::new(),
        error: Some("Error message".to_string()),
    }
    .to_bytes()?,
    new_state: Vec::new(),
})
```
