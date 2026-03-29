# 插件教程

English | [简体中文](../zh/plugin-tutorial.md)

---

## 概述

本教程逐步创建一个新的工具插件。

## 前置条件

```bash
rustup target add wasm32-unknown-unknown
```

## 步骤 1: 创建插件项目

在 `plugins/` 目录下创建新的 Rust 项目：

```bash
cargo new --target wasm32-unknown-unknown plugins/mochiclaw-my-plugin
```

## 步骤 2: 编辑 Cargo.toml

```toml
[package]
name = "mochiclaw-my-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
mochiclaw-sdk = { path = "../../mochiclaw-sdk" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
extism-pdk = "1"
```

## 步骤 3: 创建 manifest.toml

```toml
name = "mochiclaw-my-plugin"
version = "0.1.0"
description = "My custom tool plugin"

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

## 步骤 4: 实现插件

```rust
// src/lib.rs
use mochiclaw_sdk::tool::{Tool, ToolExecutionRequest, ToolExecutionResponse};
use mochiclaw_sdk::{FnResult, plugin_fn};
use std::collections::HashMap;

/// 返回此插件提供的工具列表
#[plugin_fn]
pub fn get_tools() -> FnResult<String> {
    let tools = vec![make_my_tool()];
    Ok(serde_json::to_string(&tools).unwrap())
}

/// 按名称执行工具
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

    // 你的逻辑在这里
    Ok(format!("Processed: {}", input))
}
```

## 步骤 5: 构建

```bash
cargo build --release --target wasm32-unknown-unknown -p mochi-my-plugin
```

输出: `target/wasm32-unknown-unknown/release/mochi_my_plugin.wasm`

## 步骤 6: 部署

复制 WASM 文件和 `manifest.toml` 到插件目录：

```bash
cp target/wasm32-unknown-unknown/release/mochi_my_plugin.wasm \
   ./target/plugins/
cp plugins/mochi-my-plugin/manifest.toml \
   ./target/plugins/
```

## 插件类型

### 工具插件

向 agent 提供工具：

```rust
#[plugin_fn]
pub fn get_tools() -> FnResult<String>

#[plugin_fn]
pub fn execute_tool(request: ToolExecutionRequest) -> FnResult<ToolExecutionResponse>
```

### Provider 插件

提供 LLM 访问：

```rust
#[plugin_fn]
pub fn chat(request: ChatRequest) -> FnResult<ChatResponse>

#[plugin_fn]
pub fn chat_stream(request: ChatRequest) -> FnResult<ChatResponse>  // 流式
```

### Channel 插件

处理消息：

```rust
#[plugin_fn]
pub fn poll(params: PollParams) -> FnResult<PollResponse>

#[plugin_fn]
pub fn send_text(params: SendTextParams) -> FnResult<SendResponse>

#[plugin_fn]
pub fn set_typing(params: SetTypingParams) -> FnResult<()>
```

## 使用 Host Functions

从 `mochiclaw_sdk::host` 访问能力：

```rust
use mochiclaw_sdk::host::http::{HttpClient, HttpError};
use mochiclaw_sdk::host::fs::{fs_read, fs_write};
use mochiclaw_sdk::host::kv::{kv_get, kv_set};
use mochiclaw_sdk::host::random::{rand_u32, rand_bytes};

// HTTP 请求
let resp = HttpClient::get("https://api.example.com/data").send()?;

// 文件读取
let content = fs_read("file.txt", workspace, 0, 100)?;

// KV 存储
kv_set("key", &my_value)?;
let value: MyType = kv_get("key")?;

// 随机数
let n = rand_u32();
```

## 配置获取

插件通过 `config::get()` 从主机获取配置：

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

// 或在 ToolExecutionResponse 中
Ok(ToolExecutionResponse {
    result: String::new(),
    error: Some("Error message".to_string()),
})
```
