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

## 步骤 4: 实现 Lambda（使用 #[mochi_main] 宏）

所有 lambda 使用 `#[mochi_main]` 宏，自动处理重放机制：

```rust
// src/lib.rs
use mochiclaw_sdk::lambda::{Action, Context};
use mochiclaw_sdk::tool::{Tool, ToolExecutionResponse};
use mochiclaw_macro::mochi_main;
use std::collections::HashMap;

/// Lambda 入口点 - 使用 #[mochi_main] 宏
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

## Effect 系统（HTTP + 重放机制）

**重要**：HTTP 请求通过 `ctx.http()` 自动处理，**自动支持重放**。

对于像 OpenAI 这样的 provider lambda，使用 `ctx.http()` 会自动：
1. 检查 history 是否有缓存
2. 若无缓存，抛出 `SuspendSignal` 暂停
3. 主机执行 HTTP，结果存入 history
4. 下次调用时自动从 history 恢复

```rust
// 示例：OpenAI provider 使用 ctx.http() 自动重放
fn handle_chat(ctx: &mut Context, payload: &[u8]) -> Result<Vec<u8>, SuspendSignal> {
    let input: ChatInput = rmp_serde::from_slice(payload)
        .map_err(|_| SuspendSignal::new("chat", Effect::HttpRequest(HttpEffect {
            method: String::new(),
            url: String::new(),
            headers: HashMap::new(),
            body: Some("failed to parse ChatInput".to_string()),
            timeout_ms: 0,
        })))?;

    // 构建 OpenAI 请求
    let req = HttpEffect {
        method: "POST".to_string(),
        url: "https://api.openai.com/v1/chat/completions".to_string(),
        headers: headers,
        body: Some(request_body),
        timeout_ms: 60000,
    };

    // HTTP 调用 - 第一次抛出 SuspendSignal，第二次自动返回缓存结果
    let http_result = ctx.http("chat", req)?;

    if !http_result.success {
        // 返回错误
        return Ok(rmp_serde::to_vec(&ChatResponse {
            content: String::new(),
            tool_calls: Vec::new(),
            error: http_result.error,
        }).unwrap_or_default());
    }

    // 解析响应...
    Ok(rmp_serde::to_vec(&chat_response).unwrap_or_default())
}
```

### Context 的 step() 方法

对于需要持久化的状态（如轮询游标），使用 `ctx.step()`：

```rust
// 步骤：检查缓存，若无则执行
let state: String = ctx.step("prepare_poll_state", |_ctx| {
    Ok(String::new()) // 默认值
})?;

// 更新状态（下次调用时自动从历史恢复）
let _new_state: String = ctx.step("prepare_poll_state", |_ctx| {
    Ok(new_state_value)
})?;
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
// 返回错误（通过 SuspendSignal 包装）
Err(SuspendSignal::new("step_id", Effect::HttpRequest(HttpEffect {
    method: String::new(),
    url: String::new(),
    headers: HashMap::new(),
    body: Some("Error message".to_string()),
    timeout_ms: 0,
})))

// 或者在结果中包含错误信息
Ok(rmp_serde::to_vec(&ToolExecutionResponse {
    result: String::new(),
    error: Some("Error message".to_string()),
}).unwrap_or_default())
```
