# Lambda 教程

English | [简体中文](../zh/lambda-tutorial.md)

---

## 概述

本教程逐步创建一个新的工具 lambda。

## 前置条件

```bash
rustup target add wasm32-unknown-unknown
```

## 步骤 1: 创建 Lambda 项目

在 `lambdas/` 目录下创建新的 Rust 项目：

```bash
cargo new --target wasm32-unknown-unknown lambdas/mochi-my-lambda
```

## 步骤 2: 编辑 Cargo.toml

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

## 步骤 3: 创建 manifest.toml

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

## 步骤 4: 实现 Lambda（统一入口点）

所有 lambda 使用统一的 `lambda_function` 入口点，通过 `Action` 分发：

```rust
// src/lib.rs
use mochiclaw_sdk::lambda::{Action, ExecuteToolInput, LambdaInput, LambdaOutput};
use mochiclaw_sdk::tool::{Tool, ToolExecutionResponse};
use mochiclaw_sdk::{FnResult, plugin_fn};
use std::collections::HashMap;

/// 统一的 lambda 入口点 - 处理所有 action
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

    // 你的逻辑在这里
    Ok(format!("Processed: {}", input))
}
```

## 步骤 5: 构建

```bash
cargo build --release --target wasm32-unknown-unknown -p mochi-my-lambda
```

输出: `target/wasm32-unknown-unknown/release/mochi_my_lambda.wasm`

## 步骤 6: 部署

复制 WASM 文件和 `manifest.toml` 到 lambda 目录：

```bash
cp target/wasm32-unknown-unknown/release/mochi_my_lambda.wasm \
   ./target/lambdas/
cp lambdas/mochi-my-lambda/manifest.toml \
   ./target/lambdas/
```

## Lambda 类型与 Action

所有 lambda 使用 `lambda_function` 通过 `Action` 分发。支持的 action 取决于 lambda 类型：

### Tool Lambda

| Action | 输入 | 输出 |
|--------|------|------|
| `GetTools` | 无 | `String`（JSON 工具数组） |
| `ExecuteTool` | `ExecuteToolInput` | `ToolExecutionResponse` |

### Provider Lambda

| Action | 输入 | 输出 |
|--------|------|------|
| `Chat` | `ChatInput` | `ChatResponse` |

### Channel Lambda

| Action | 输入 | 输出 |
|--------|------|------|
| `PreparePoll` | `PreparePollInput` | `DigestOutput` |
| `FormatSend` | `SendInput` | `SendOutput` |
| `SetTyping` | `SetTypingInput` | `()` |
| `Login` | `LoginInput` | `LoginOutput` |
| `CheckLogin` | `CheckLoginInput` | `CheckLoginOutput` |

## Effect 系统（HTTP）

**重要**：HTTP 不再是直接的 host function。Lambda 返回 `HttpEffect` 声明，由主机执行。

对于像 OpenAI 这样的 provider lambda，`Chat` action 通常涉及两次调用：

1. **第一次调用**：返回 `HttpEffect` 用于 API 请求
2. **第二次调用**：携带 `effect_results`（包含 HTTP 响应），解析并返回 `ChatResponse`

```rust
// 示例：OpenAI provider 返回 HttpEffect
fn handle_chat(params: LambdaInput) -> FnResult<LambdaOutput> {
    if !params.effect_results.is_empty() {
        // 第二次调用：解析 HTTP 响应
        let response = &params.effect_results[0];
        let chat_response = parse_openai_response(response)?;
        return Ok(LambdaOutput {
            effects: vec![],
            result: chat_response.to_bytes()?,
            new_state: Vec::new(),
        });
    }

    // 第一次调用：返回 HTTP effect
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

## 使用 Host Functions

从 `mochiclaw_sdk::host` 访问能力：

```rust
use mochiclaw_sdk::host::fs::{fs_read, fs_write};
use mochiclaw_sdk::host::kv::{kv_get, kv_set};
use mochiclaw_sdk::host::random::{rand_u32, rand_bytes};

// 文件读取
let content = fs_read("file.txt", workspace, 0, 100)?;

// KV 存储
kv_set("key", &my_value)?;
let value: MyType = kv_get("key")?;

// 随机数
let n = rand_u32();
```

## 配置获取

Lambda 通过 `config::get()` 从主机获取配置：

```rust
use mochiclaw_sdk::config;

// 获取 workspace（由主机注入）
let workspace = match config::get("workspace") {
    Ok(Some(w)) => w,
    _ => ".".to_string(),
};
```

## 错误处理

```rust
// 在工具执行中返回错误
Err("Something went wrong".into())

// 或在 LambdaOutput 中使用 error 字段
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
