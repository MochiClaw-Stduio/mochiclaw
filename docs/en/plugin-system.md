# Plugin System

[简体中文](../zh/plugin-system.md) | English

---

## Overview

Mochiclaw's plugin system is built on **Extism**, allowing plugins to run as **WASM (WASM32-unknown-unknown)** in an isolated sandbox environment. Each plugin is completely isolated and can only interact with the outside world through explicitly declared capabilities.

## Core Concepts

### 1. Plugin Types (Features)

Plugins can declare the following feature types:

| Type | Description | Example |
|------|-------------|---------|
| `provider` | LLM Provider, provides AI model calling capability | `mochi-openai` |
| `channel` | Message channel, handles user message I/O | `mochi-weixin` |
| `tool` | Tools, provides various auxiliary capabilities | `mochi-fs` |
| `command` | Commands, handles specific commands | - |

### 2. Capability System (Capabilities)

Plugins must declare their required capabilities, and the system enforces access control based on these declarations:

#### Network Capabilities

```toml
[capabilities.network]
enabled = true
allowed_hosts = ["*.openai.com", "api.deepseek.com"]
denied_hosts = ["evil.com"]
```

- `enabled`: Whether network access is enabled
- `allowed_hosts`: Allowed HTTP hosts (glob patterns supported)
- `denied_hosts`: Denied HTTP hosts (takes precedence over allowed_hosts)

#### Filesystem Capabilities (FS)

```toml
[capabilities.fs]
enabled = true
allowed_root = "${workspace}"      # Sandbox root directory
read_whitelist = ["${workspace}"] # Allowed read paths
write_whitelist = ["${workspace}"]# Allowed write paths
read_blacklist = []                # Read blacklist
write_blacklist = []               # Write blacklist
```

- `allowed_root`: Root directory for filesystem operations (sandbox boundary), supports `${workspace}` placeholder
- `read_whitelist`/`write_whitelist`: Path whitelists
- `read_blacklist`/`write_blacklist`: Path blacklists (take precedence over whitelists)

#### KV Storage Capabilities

```toml
[capabilities]
allowed_kv_read = ["plugin-a", "plugin-b"]  # Can read other plugins' KV
```

- Each plugin has its own KV namespace
- By default, can only read/write its own KV
- Can read other plugins' KV through `allowed_kv_read`

### 3. Plugin Manifest

Each plugin requires a `manifest.toml` file:

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

## Runtime Architecture

```
┌───────────────────────────────────────────────────────────┐
│                     PluginHost                            │
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

### PluginHost

`PluginHost` is the plugin runtime manager:

- **CompiledPlugin Pool**: Each plugin has one JIT-compiled `CompiledPlugin`
- **Pool**: Each plugin has one `Pool` managing multiple `Plugin` instances for concurrency
- **Host Functions**: Capabilities exposed to plugins (HTTP, FS, KV, Rand)

### Concurrency Model

Plugins achieve concurrency through **Pool**:

```rust
let pool = PoolBuilder::new()
    .with_max_instances(std::thread::available_parallelism().unwrap().into())
    .build(move || {
        Plugin::new_from_compiled(&compiled)
    });
```

When calling a plugin, acquire an instance from the pool:

```rust
let mut plugin = pool.get(timeout)?;
plugin.call("function_name", &input)?
```

## Host Functions

### HTTP

```rust
// mochiclaw-sdk/src/host/http.rs
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

### Filesystem

```rust
// mochiclaw-sdk/src/host/fs.rs
fn fs_read(path: &str) -> Result<String, KvError>
fn fs_write(path: &str, content: &str) -> Result<(), KvError>
fn fs_edit(path: &str, old: &str, new: &str) -> Result<(), KvError>
fn fs_list(path: &str) -> Result<Vec<String>, KvError>
```

### KV Storage

```rust
// mochiclaw-sdk/src/host/kv.rs
fn kv_set(key: &str, value: &[u8]) -> Result<(), KvError>
fn kv_get(key: &str) -> Result<Vec<u8>, KvError>
fn kv_remove(key: &str) -> Result<(), KvError>
fn kv_set_raw(plugin: &str, key: &str, value: &[u8]) -> Result<(), KvError>
fn kv_get_raw(plugin: &str, key: &str) -> Result<Vec<u8>, KvError>
```

### Random

```rust
// mochiclaw-sdk/src/host/random.rs
fn rand_u32() -> u32
fn rand_u64() -> u64
fn rand_u32_bounded(max: u32) -> u32
fn rand_bytes(n: u32) -> Vec<u8>
```

## Plugin Development

### 1. Create Plugin Project

```bash
# Create in plugins/ directory
cargo new --target wasm32-unknown-unknown my-plugin
```

### 2. Write manifest.toml

```toml
name = "mochi-my-plugin"
version = "0.1.0"
description = "My custom plugin"

[capabilities.network]
enabled = true
allowed_hosts = ["api.example.com"]

[capabilities.fs]
enabled = true
allowed_root = "${workspace}"

[features]
tool = true
```

### 3. Implement Plugin Logic

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

### 4. Build

```bash
cargo build --release --target wasm32-unknown-unknown -p mochi-my-plugin
```

### 5. Deploy

Copy `target/wasm32-unknown-unknown/release/mochi_my_plugin.wasm` and `manifest.toml` to the plugin directory.

## Configuration Override

Users can override plugin-declared capabilities in `config.toml`:

```toml
[plugins.mochi-openai]
[plugins.mochi-openai.capabilities.network]
allowed_hosts = ["*.openai.com", "*.deepseek.com"]

[plugins.mochi-fs]
[plugins.mochi-fs.capabilities.fs]
allowed_root = "/custom/path"
```

Override rules:
- **Booleans**: `enabled` and other boolean fields, user config takes precedence
- **Strings**: `allowed_root` and other string fields, user config takes precedence
- **Lists**: Whitelists/blacklists, user config is appended to declared config

## Security Model

1. **Sandbox Isolation**: WASM plugins run in an independent virtual machine
2. **Capability Declaration**: Plugins must declare required capabilities
3. **Access Control**: Network hosts and filesystem paths both support blacklists
4. **KV Isolation**: By default, can only access own KV storage
