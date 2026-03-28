# Host Functions

English | [简体中文](../zh/host-functions.md)

---

## 概述

Host functions 是运行时暴露给 WASM 插件的能力。插件通过 `mochiclaw_sdk::host::*` 模块访问。

## HTTP

插件可以向允许的主机发送 HTTP 请求。

```rust
use mochiclaw_sdk::host::http::{HttpClient, HttpError};

// GET 请求
let response = HttpClient::get("https://api.example.com/data")
    .header("Authorization", "Bearer token")
    .send()?;

// POST 请求（JSON body）
let response = HttpClient::post("https://api.example.com/data")
    .json(&payload)?
    .send()?;
```

### HttpClient 方法

| 方法 | 描述 |
|------|------|
| `HttpClient::new()` | 创建新客户端（默认 GET） |
| `HttpClient::get(url)` | 开始 GET 请求 |
| `HttpClient::post(url)` | 开始 POST 请求 |
| `HttpClient::put(url)` | 开始 PUT 请求 |
| `HttpClient::delete(url)` | 开始 DELETE 请求 |
| `HttpClient::patch(url)` | 开始 PATCH 请求 |
| `HttpClient::head(url)` | 开始 HEAD 请求 |
| `.url(url)` | 设置 URL |
| `.method(verb)` | 设置 HTTP 方法 |
| `.header(key, value)` | 添加 header |
| `.json(value)` | 设置 JSON body（自动设置 Content-Type） |
| `.body(bytes)` | 设置原始 body |
| `.send()` | 发送请求 |

### HttpResponse 方法

```rust
let response = HttpClient::get("https://api.example.com").send()?;

// 解析为文本
let text = response.text()?;

// 解析为 JSON
let data: MyType = response.json()?;

// 原始字节
let bytes = response.bytes();
```

| 方法 | 返回值 |
|------|--------|
| `text()` | `Result<&str>` |
| `json<T>()` | `Result<T>` (JSON 反序列化) |
| `bytes()` | `&[u8]` |

### HttpError 变体

```rust
pub enum HttpError {
    Request(String),      // 请求失败
    StatusCode(u16),      // HTTP 错误状态码
    InvalidUtf8,          // 响应不是 UTF-8
    Json(serde_json::Error), // JSON 解析错误
}
```

## 文件系统

文件系统访问被限制在 `allowed_root` 内，并通过白名单/黑名单控制。

```rust
use mochiclaw_sdk::host::fs::{fs_read, fs_write, fs_edit, fs_list};

// 读取文件（行号从 1 开始，0 = 全部）
let content = fs_read("src/main.rs", workspace, 1, 100)?;

// 写入文件
fs_write("output.txt", workspace, "hello world")?;

// 编辑文件（替换第一个匹配）
fs_edit("file.rs", workspace, "old text", "new text", false)?;

// 替换所有匹配
fs_edit("file.rs", workspace, "old", "new", true)?;

// 列出目录
let listing = fs_list(".", workspace, false, 100)?;
```

### fs_read

```rust
fn fs_read(
    path: &str,        // 文件路径
    workspace: &str,    // 用于解析相对路径的工作目录
    offset: u64,       // 行偏移（从 1 开始，0 = 从头）
    limit: u64,        // 最大行数（0 = 无限制）
) -> Result<String, String>
```

### fs_write

```rust
fn fs_write(
    path: &str,
    workspace: &str,
    content: &str,
) -> Result<bool, String>
```

### fs_edit

```rust
fn fs_edit(
    path: &str,
    workspace: &str,
    old_text: &str,     // 要查找的文本
    new_text: &str,     // 替换文本
    replace_all: bool,  // 替换所有匹配项
) -> Result<String, String>  // 成功消息
```

### fs_list

```rust
fn fs_list(
    path: &str,
    workspace: &str,
    recursive: bool,
    max_entries: u64,
) -> Result<String, String>  // 换行分隔的列表
```

## KV 存储

插件私有的键值存储，支持可选的跨插件读取访问。

```rust
use mochiclaw_sdk::host::kv::{kv_get, kv_set, kv_remove, kv_get_raw};

// 存储值（序列化为 MessagePack）
kv_set("counter", &0u32)?;
kv_set("user", &User { name: "Alice".to_string() })?;

// 检索值
let counter: u32 = kv_get("counter")?;
let user: User = kv_get("user")?;

// 删除
kv_remove("temp")?;

// 原始字节（无序列化）
kv_set_raw("binary", vec![1, 2, 3])?;
let raw = kv_get_raw("binary")?;
```

### KV 函数

| 函数 | 描述 |
|------|------|
| `kv_set(key, value)` | 存储可序列化值 |
| `kv_get<T>(key)` | 检索并反序列化值 |
| `kv_set_raw(key, bytes)` | 存储原始字节 |
| `kv_get_raw(key)` | 获取原始字节 |
| `kv_remove(key)` | 删除键 |
| `kv_list_readable()` | 列出此插件可读取的插件 |
| `kv_list_writable()` | 列出此插件可写入的插件 |

### 跨插件 KV 访问

如果插件在能力声明中有 `allowed_kv_read = ["plugin-a", "plugin-b"]`：

```rust
use mochiclaw_sdk::host::kv::kv_get_from;

// 从另一个插件的 KV 读取
let value: SomeType = kv_get_from("plugin-a", "shared_key")?;
```

## 随机数

密码学安全的随机数生成。

```rust
use mochiclaw_sdk::host::random::{rand_u32, rand_u64, rand_bytes, rand_u32_bounded};

// 随机 u32
let n: u32 = rand_u32();

// 随机 u64
let n: u64 = rand_u64();

// 随机字节
let mut buf = vec![0u8; 32];
rand_bytes(&mut buf);

// 范围内的随机数 [0, end)
let n: u32 = rand_u32_bounded(100);  // 0-99
```

### Random 函数

| 函数 | 返回值 | 范围 |
|------|--------|------|
| `rand_u32()` | u32 | 全范围 |
| `rand_u64()` | u64 | 全范围 |
| `rand_bytes(buf)` | () | 填充缓冲区 |
| `rand_u32_bounded(end)` | u32 | [0, end) |
