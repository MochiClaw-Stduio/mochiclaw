# Host Functions

[简体中文](../zh/host-functions.md) | English

---

## Overview

Host functions are capabilities exposed to WASM lambdas by the runtime. Lambdas access them through the `mochiclaw_sdk::host::*` module.

**Note**: HTTP is no longer a host function. Lambdas declare HTTP as `HttpEffect` in their return value, and the host's `AsyncHttpExecutor` handles execution with permission enforcement. See [Effect System](../en/lambda-system.md#effect-system-http-execution) for details.

## Filesystem

Filesystem access is sandboxed to `allowed_root` and controlled by whitelist/blacklist.

```rust
use mochiclaw_sdk::host::fs::{fs_read, fs_write, fs_edit, fs_list};

// Read file (1-indexed lines, 0 = all)
let content = fs_read("src/main.rs", workspace, 1, 100)?;

// Write file
fs_write("output.txt", workspace, "hello world")?;

// Edit file (first occurrence)
fs_edit("file.rs", workspace, "old text", "new text", false)?;

// Replace all occurrences
fs_edit("file.rs", workspace, "old", "new", true)?;

// List directory
let listing = fs_list(".", workspace, false, 100)?;
```

### fs_read

```rust
fn fs_read(
    path: &str,        // File path
    workspace: &str,   // Workspace for resolving relative paths
    offset: u64,       // Line offset (1-indexed, 0 = start)
    limit: u64,        // Max lines (0 = unlimited)
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
    old_text: &str,     // Text to find
    new_text: &str,     // Replacement
    replace_all: bool,  // Replace all occurrences
) -> Result<String, String>  // Success message
```

### fs_list

```rust
fn fs_list(
    path: &str,
    workspace: &str,
    recursive: bool,
    max_entries: u64,
) -> Result<String, String>  // Newline-separated list
```

## KV Storage

Lambda-private key-value store with optional cross-lambda read access.

```rust
use mochiclaw_sdk::host::kv::{kv_get, kv_set, kv_remove, kv_get_raw};

// Store values (serializes to MessagePack)
kv_set("counter", &0u32)?;
kv_set("user", &User { name: "Alice".to_string() })?;

// Retrieve values
let counter: u32 = kv_get("counter")?;
let user: User = kv_get("user")?;

// Remove
kv_remove("temp")?;

// Raw bytes (no serialization)
kv_set_raw("binary", vec![1, 2, 3])?;
let raw = kv_get_raw("binary")?;
```

### KV Functions

| Function | Description |
|----------|-------------|
| `kv_set(key, value)` | Store serializable value |
| `kv_get<T>(key)` | Retrieve and deserialize value |
| `kv_set_raw(key, bytes)` | Store raw bytes |
| `kv_get_raw(key)` | Get raw bytes |
| `kv_remove(key)` | Delete key |
| `kv_list_readable()` | List lambdas readable by this lambda |
| `kv_list_writable()` | List lambdas writable by this lambda |

### Cross-Lambda KV Access

If a lambda has `allowed_kv_read = ["lambda-a", "lambda-b"]` in its capabilities:

```rust
use mochiclaw_sdk::host::kv::kv_get_from;

// Read from another lambda's KV
let value: SomeType = kv_get_from("lambda-a", "shared_key")?;
```

## Random

Cryptographically secure random number generation.

```rust
use mochiclaw_sdk::host::random::{rand_u32, rand_u64, rand_bytes, rand_u32_bounded};

// Random u32
let n: u32 = rand_u32();

// Random u64
let n: u64 = rand_u64();

// Random bytes
let mut buf = vec![0u8; 32];
rand_bytes(&mut buf);

// Random in range [0, end)
let n: u32 = rand_u32_bounded(100);  // 0-99
```

### Random Functions

| Function | Returns | Range |
|----------|---------|-------|
| `rand_u32()` | u32 | Full range |
| `rand_u64()` | u64 | Full range |
| `rand_bytes(buf)` | () | Fills buffer |
| `rand_u32_bounded(end)` | u32 | [0, end) |
