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

## Step 4: Implement Lambda with #[mochi_main] Macro

All lambdas use `#[mochi_main]` macro with automatic replay support:

```rust
// src/lib.rs
use mochiclaw_sdk::lambda::{Action, Context};
use mochiclaw_sdk::tool::{Tool, ToolExecutionResponse};
use mochiclaw_macro::mochi_main;
use std::collections::HashMap;

/// Lambda entry point - using #[mochi_main] macro
#[mochi_main]
pub fn main_handler(
    ctx: &mut Context,
    action: Action,
    payload: &[u8],
) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    match action {
        Action::GetTools => handle_get_tools(ctx),
        Action::ExecuteTool => handle_execute_tool(ctx, payload),
        _ => Ok(Vec::new()),
    }
}

fn handle_get_tools(_ctx: &mut Context) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    let tools = vec![make_my_tool()];
    let tools_json = serde_json::to_string(&tools).unwrap();
    Ok(rmp_serde::to_vec(&tools_json).unwrap_or_default())
}

fn handle_execute_tool(
    ctx: &mut Context,
    payload: &[u8],
) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    let input: mochiclaw_sdk::lambda::ExecuteToolInput = rmp_serde::from_slice(payload)
        .map_err(|_| mochiclaw_sdk::lambda::SuspendSignal::new("parse_error",
            mochiclaw_sdk::lambda::Effect::HttpRequest(mochiclaw_sdk::lambda::HttpEffect {
                method: String::new(),
                url: String::new(),
                headers: HashMap::new(),
                body: Some("failed to parse ExecuteToolInput".to_string()),
                timeout_ms: 0,
            })
        ))?;

    let result = match input.name.as_str() {
        "my_tool" => do_something(&input.arguments)?,
        _ => return Ok(rmp_serde::to_vec(&ToolExecutionResponse {
            result: String::new(),
            error: Some(format!("Unknown tool: {}", input.name)),
        }).unwrap_or_default()),
    };

    Ok(rmp_serde::to_vec(&ToolExecutionResponse {
        result,
        error: None,
    }).unwrap_or_default())
}
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

## Effect System (HTTP + Replay Mechanism)

**Important**: HTTP requests are handled via `ctx.http()` which **automatically supports replay**.

For provider lambdas like OpenAI, using `ctx.http()` automatically:
1. Checks history for cached result
2. If not cached, throws `SuspendSignal` to suspend
3. Host executes HTTP, stores result in history
4. On next call, automatically resumes from history

```rust
// Example: OpenAI provider using ctx.http() for automatic replay
fn handle_chat(ctx: &mut Context, payload: &[u8]) -> Result<Vec<u8>, SuspendSignal> {
    let input: ChatInput = rmp_serde::from_slice(payload)
        .map_err(|_| SuspendSignal::new("chat", Effect::HttpRequest(HttpEffect {
            method: String::new(),
            url: String::new(),
            headers: HashMap::new(),
            body: Some("failed to parse ChatInput".to_string()),
            timeout_ms: 0,
        })))?;

    // Build OpenAI request
    let req = HttpEffect {
        method: "POST".to_string(),
        url: "https://api.openai.com/v1/chat/completions".to_string(),
        headers: headers,
        body: Some(request_body),
        timeout_ms: 60000,
    };

    // HTTP call - first time throws SuspendSignal, second time returns cached result
    let http_result = ctx.http("chat", req)?;

    if !http_result.success {
        // Return error
        return Ok(rmp_serde::to_vec(&ChatResponse {
            content: String::new(),
            tool_calls: Vec::new(),
            error: http_result.error,
        }).unwrap_or_default());
    }

    // Parse response...
    Ok(rmp_serde::to_vec(&chat_response).unwrap_or_default())
}
```

### Context step() Method

For persistent state (like poll cursors), use `ctx.step()`:

```rust
// Step: check cache, execute if miss
let state: String = ctx.step("prepare_poll_state", |_ctx| {
    Ok(String::new()) // default value
})?;

// Update state (auto-restored from history on next call)
let _new_state: String = ctx.step("prepare_poll_state", |_ctx| {
    Ok(new_state_value)
})?;
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
// Return error (wrapped in SuspendSignal)
Err(SuspendSignal::new("step_id", Effect::HttpRequest(HttpEffect {
    method: String::new(),
    url: String::new(),
    headers: HashMap::new(),
    body: Some("Error message".to_string()),
    timeout_ms: 0,
})))

// Or include error in result
Ok(rmp_serde::to_vec(&ToolExecutionResponse {
    result: String::new(),
    error: Some("Error message".to_string()),
}).unwrap_or_default())
```
