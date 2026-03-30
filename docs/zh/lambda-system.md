# Lambda 系统

[English](../en/lambda-system.md) | 简体中文

---

## 概述

Mochiclaw 的 lambda 系统基于 **Extism** 构建，允许 lambda 以 **WASM（WASM32-unknown-unknown）** 格式运行在独立的沙箱环境中。每个 lambda 完全隔离，只能通过显式声明的能力（Capabilities）与外界交互。

**关键架构变化**：HTTP 执行已从 lambda 内部移至主机层，通过 **Effect 系统**实现。Lambda 返回 `HttpEffect` 声明，主机的 `AsyncHttpExecutor` 处理实际 HTTP 执行并进行权限检查。

## 核心概念

### 1. Lambda 类型（Features）

Lambda 可以声明以下功能类型：

| 类型 | 说明 | 示例 |
|------|------|------|
| `provider` | LLM Provider，提供 AI 模型调用能力 | `mochi-openai` |
| `channel` | 消息通道，处理用户消息输入输出 | `mochi-weixin` |
| `tool` | 工具，提供各种辅助能力 | `mochi-fs` |
| `command` | 命令，处理特定命令 | - |

### 2. 能力系统（Capabilities）

Lambda 必须声明其需要的能力，系统根据声明进行访问控制：

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
allowed_kv_read = ["lambda-a", "lambda-b"]  # 可读取其他 lambda 的 KV
```

- 每个 lambda 有自己的 KV 命名空间
- 默认只能读写自己的 KV
- 通过 `allowed_kv_read` 可以读取其他 lambda 的 KV

### 3. Lambda 清单（Manifest）

每个 lambda 需要一个 `manifest.toml` 文件：

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
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐               │   │
│  │  │   FS    │ │   KV    │ │  Rand   │               │   │
│  │  └─────────┘ └─────────┘ └─────────┘               │   │
│  └────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────┘
                          │
                          │ lambda_call() 返回 effects
                          ▼
┌───────────────────────────────────────────────────────────┐
│              AsyncHttpExecutor (主机层)                   │
│  - 执行 HttpEffect 并进行权限检查                         │
│  - 代理支持                                               │
│  - 通过 futures::join_all 并行执行                        │
└───────────────────────────────────────────────────────────┘
```

### LambdaHost

`LambdaHost` 是 lambda 的运行时管理器：

- **CompiledPlugin 池**：每个 lambda 有一个 JIT 编译的 `CompiledPlugin`（跨 pool 实例共享）
- **Pool**：每个 lambda 有一个 `Pool`，管理多个 `Plugin` 实例实现并发
- **Host Functions**：向 lambda 暴露的能力（FS、KV、Rand）

### 并发模型

Lambda 通过 **Pool** 实现并发：

```rust
let pool = PoolBuilder::new()
    .with_max_instances(std::thread::available_parallelism().unwrap().into())
    .build(move || {
        Plugin::new_from_compiled(&compiled)
    });
```

调用 lambda 时从池中获取实例：

```rust
let mut lambda = pool.get(timeout)?;
lambda.call("function_name", &input)?
```

## Effect 系统（HTTP 执行）

Lambda 不再直接发起 HTTP 请求，而是返回 `HttpEffect` 声明。主机 的 `AsyncHttpExecutor` 处理执行：

### HttpEffect 结构

```rust
struct HttpEffect {
    method: String,                    // GET, POST, PUT, DELETE 等
    url: String,                       // 完整 URL
    headers: HashMap<String, String>, // 请求头
    body: Option<String>,              // 请求体
    timeout_ms: u32,                   // 超时时间（毫秒）
}
```

### Effect 流程（重放机制）

```
Lambda (with Context)              Host                            外部
  │                                   │                                │
  │ lambda_main(LambdaInput)          │                                │
  │◄──────────────────────────────────│                                │
  │                                   │                                │
  │ ctx.http() -> Suspend            │                                │
  │ LambdaOutput::Suspended {         │                                │
  │   effect: HttpEffect,             │                                │
  │   step_id: "chat_1",             │                                │
  │   new_history: {}                 │                                │
  │ }                                 │                                │
  │──────────────────────────────────►│                                │
  │                         AsyncHttpExecutor                          │
  │                         .execute_effect()                         │
  │                                   │                                │
  │                         HTTP Request ────────────────────────────► │
  │                                   │                                │
  │                    EffectResult { success, response }              │
  │◄──────────────────────────────────│                                │
  │                                   │                                │
  │                         history_store.merge(step_id, result)       │
  │                                   │                                │
  │ lambda_main(LambdaInput)          │                                │
  │   (with history)                  │                                │
  │◄──────────────────────────────────│                                │
  │                                   │                                │
  │ ctx.http() -> cached result      │                                │
  │ LambdaOutput::Finished {         │                                │
  │   result: ChatResponse {...}      │                                │
  │ }                                 │                                │
  │──────────────────────────────────►│ (return to caller)             │
```

## 宿主函数（Host Functions）

### 文件系统

```rust
// sdk/src/host/fs.rs
fn fs_read(path: &str, workspace: &str, offset: u64, limit: u64) -> Result<String, String>
fn fs_write(path: &str, workspace: &str, content: &str) -> Result<bool, String>
fn fs_edit(path: &str, workspace: &str, old: &str, new: &str, replace_all: bool) -> Result<String, String>
fn fs_list(path: &str, workspace: &str, recursive: bool, max_entries: u64) -> Result<String, String>
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

## Lambda 开发

### 1. 创建 Lambda 项目

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

### 3. 实现 Lambda 逻辑（使用 #[mochi_main] 宏）

所有 lambda 使用 `#[mochi_main]` 宏，自动处理重放：

```rust
use mochiclaw_sdk::lambda::{Action, Context, SuspendSignal};
use mochiclaw_macro::mochi_main;

#[mochi_main]
pub fn main_handler(
    ctx: &mut Context,
    action: Action,
    payload: &[u8],
) -> Result<Vec<u8>, SuspendSignal> {
    match action {
        Action::GetTools => handle_get_tools(ctx),
        Action::ExecuteTool => handle_execute_tool(ctx, payload),
        _ => Ok(Vec::new()),
    }
}
```

### 4. 构建

```bash
cargo build --release --target wasm32-unknown-unknown -p mochi-my-lambda
```

### 5. 部署

将 `target/wasm32-unknown-unknown/release/mochi_my_lambda.wasm` 和 `manifest.toml` 复制到 lambda 目录。

## 配置覆盖

用户可以在 `config.toml` 中覆盖 lambda 声明的能力：

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

1. **沙箱隔离**：WASM lambda 运行在独立的虚拟机中
2. **能力声明**：lambda 必须声明所需能力
3. **基于 Effect 的 HTTP**：网络请求声明为 effects，由主机进行权限检查后执行
4. **访问控制**：网络 hosts、文件系统路径均支持黑名单
5. **KV 隔离**：默认只能访问自己的 KV 存储
