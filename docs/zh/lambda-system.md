# 插件系统

[English](../en/lambda-system.md) | 简体中文

---

## 概述

Mochiclaw 的插件系统基于 **Extism** 构建，允许插件以 **WASM（WASM32-unknown-unknown）** 格式运行在独立的沙箱环境中。每个插件完全隔离，只能通过显式声明的能力（Capabilities）与外界交互。

## 核心概念

### 1. 插件类型（Features）

插件可以声明以下功能类型：

| 类型 | 说明 | 示例 |
|------|------|------|
| `provider` | LLM Provider，提供 AI 模型调用能力 | `mochi-openai` |
| `channel` | 消息通道，处理用户消息输入输出 | `mochi-weixin` |
| `tool` | 工具，提供各种辅助能力 | `mochi-fs` |
| `command` | 命令，处理特定命令 | - |

### 2. 能力系统（Capabilities）

插件必须声明其需要的能力，系统根据声明进行访问控制：

#### 网络能力（Network）

```toml
[capabilities.network]
enabled = true
allowed_hosts = ["*.openai.com", "api.deepseek.com"]
denied_hosts = ["evil.com"]
```

- `enabled`：是否启用网络访问
- `allowed_hosts`：允许访问的 hosts（glob 模式支持）
- `denied_hosts`：禁止访问的 hosts（优先级高于 allowed_hosts）

#### 文件系统能力（FS）

```toml
[capabilities.fs]
enabled = true
allowed_root = "${workspace}"      # 沙箱根目录
read_whitelist = ["${workspace}"] # 允许读取的路径
write_whitelist = ["${workspace}"]# 允许写入的路径
read_blacklist = []                # 读取黑名单
write_blacklist = []               # 写入黑名单
```

- `allowed_root`：文件系统操作的根目录（沙箱边界），支持 `${workspace}` 占位符
- `read_whitelist`/`write_whitelist`：路径白名单
- `read_blacklist`/`write_blacklist`：路径黑名单（优先级高于白名单）

#### KV 存储能力

```toml
[capabilities]
allowed_kv_read = ["lambda-a", "lambda-b"]  # 可读取其他插件的 KV
```

- 每个插件有自己的 KV 命名空间
- 默认只能读写自己的 KV
- 通过 `allowed_kv_read` 可以读取其他插件的 KV

### 3. 插件清单（Manifest）

每个插件需要一个 `manifest.toml` 文件：

```toml
name = "mochi-fs"
version = "0.1.0"
description = "Filesystem tools: read_file, write_file, edit_file, list_dir"

[capabilities.network]
enabled = false

[capabilities.fs]
enabled = true
allowed_root = "${workspace}"
read_whitelist = ["${workspace}"]
write_whitelist = ["${workspace}"]

[features]
tool = true
```

## 运行时架构

```
┌───────────────────────────────────────────────────────────┐
│                     LambdaHost                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │  Pool       │  │  Pool       │  │  Pool       │        │
│  │  (openai)   │  │  (fs)       │  │  (weixin)   │        │
│  │  ┌───────┐  │  │  ┌───────┐  │  │  ┌───────┐  │        │
│  │  │Inst 1 │  │  │  │Inst 1 │  │  │  │Inst 1 │  │        │
│  │  │Inst 2 │  │  │  │Inst 2 │  │  │  │Inst 2 │  │        │
│  │  │  ...  │  │  │  │  ...  │  │  │  │  ...  │  │        │
│  │  └───────┘  │  │  └───────┘  │  │  └───────┘  │        │
│  └─────────────┘  └─────────────┘  └─────────────┘        │
│                                                           │
│  ┌────────────────────────────────────────────────────┐   │
│  │              Host Functions                        │   │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐   │   │
│  │  │   HTTP  │ │   FS    │ │   KV    │ │  Rand   │   │   │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘   │   │
│  └────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────┘
```

### LambdaHost

`LambdaHost` 是插件的运行时管理器：

- **CompiledPlugin 池**：每个插件有一个 JIT 编译的 `CompiledPlugin`
- **Pool**：每个插件有一个 `Pool`，管理多个 `Lambda` 实例实现并发
- **Host Functions**：向插件暴露的能力（HTTP、FS、KV、Rand）

### 并发模型

插件通过 **Pool** 实现并发：

```rust
let pool = PoolBuilder::new()
    .with_max_instances(std::thread::available_parallelism().unwrap().into())
    .build(move || {
        Lambda::new_from_compiled(&compiled)
    });
```

插件调用时从池中获取实例：

```rust
let mut lambda = pool.get(timeout)?;
lambda.call("function_name", &input)?
```

## 宿主函数（Host Functions）

### HTTP

```rust
// sdk/src/host/http.rs
pub struct HttpClient {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
}

pub struct HttpResponse {
    pub status: u32,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}
```

### 文件系统

```rust
// sdk/src/host/fs.rs
fn fs_read(path: &str) -> Result<String, KvError>
fn fs_write(path: &str, content: &str) -> Result<(), KvError>
fn fs_edit(path: &str, old: &str, new: &str) -> Result<(), KvError>
fn fs_list(path: &str) -> Result<Vec<String>, KvError>
```

### KV 存储

```rust
// sdk/src/host/kv.rs
fn kv_set(key: &str, value: &[u8]) -> Result<(), KvError>
fn kv_get(key: &str) -> Result<Vec<u8>, KvError>
fn kv_remove(key: &str) -> Result<(), KvError>
fn kv_set_raw(lambda: &str, key: &str, value: &[u8]) -> Result<(), KvError>
fn kv_get_raw(lambda: &str, key: &str) -> Result<Vec<u8>, KvError>
```

### 随机数

```rust
// sdk/src/host/random.rs
fn rand_u32() -> u32
fn rand_u64() -> u64
fn rand_u32_bounded(max: u32) -> u32
fn rand_bytes(n: u32) -> Vec<u8>
```

## 插件开发

### 1. 创建插件项目

```bash
# 在 lambdas/ 目录下创建
cargo new --target wasm32-unknown-unknown my-lambda
```

### 2. 编写 manifest.toml

```toml
name = "mochi-my-lambda"
version = "0.1.0"
description = "My custom lambda"

[capabilities.network]
enabled = true
allowed_hosts = ["api.example.com"]

[capabilities.fs]
enabled = true
allowed_root = "${workspace}"

[features]
tool = true
```

### 3. 实现插件逻辑

```rust
use mochiclaw_sdk::*;

#[plugin_fn]
pub fn execute_tool(input: ToolExecutionRequest) -> FnResult<ToolExecutionResponse> {
    let name = input.name;
    let args = input.arguments;

    let result = match name.as_str() {
        "my_tool" => do_something(args),
        _ => return Err(制.into()),
    };

    Ok(ToolExecutionResponse {
        result,
        error: None,
    })
}
```

### 4. 构建

```bash
cargo build --release --target wasm32-unknown-unknown -p mochi-my-lambda
```

### 5. 部署

将 `target/wasm32-unknown-unknown/release/mochi_my_lambda.wasm` 和 `manifest.toml` 复制到插件目录。

## 配置覆盖

用户可以在 `config.toml` 中覆盖插件声明的能力：

```toml
[lambdas.mochi-openai]
[lambdas.mochi-openai.capabilities.network]
allowed_hosts = ["*.openai.com", "*.deepseek.com"]

[lambdas.mochi-fs]
[lambdas.mochi-fs.capabilities.fs]
allowed_root = "/custom/path"
```

覆盖规则：
- **布尔值**：`enabled` 等布尔字段，用户配置优先
- **字符串**：`allowed_root` 等字符串字段，用户配置优先
- **列表**：白名单/黑名单，用户配置会追加到声明配置之后

## 安全模型

1. **沙箱隔离**：WASM 插件运行在独立的虚拟机中
2. **能力声明**：插件必须声明所需能力
3. **访问控制**：网络 hosts、文件系统路径均支持黑名单
4. **KV 隔离**：默认只能访问自己的 KV 存储
