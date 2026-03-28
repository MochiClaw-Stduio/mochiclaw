# Configuration Reference

[简体中文](../zh/configuration.md) | English

---

## Overview

Mochiclaw uses `config.toml` for all configuration. This document describes all available options.

## Full Example

```toml
version = 1

[agent]
model = "deepseek-chat"
max_iterations = 40
workspace = "./workspace"

[runtime]
plugin_dirs = ["./target/plugins"]

[runtime.log]
level = "info"
dir = ".logs"
max_age_days = 30

[runtime.network]
use_system_proxy = false

[plugins.mochiclaw-openai]
proxy_url = "http://127.0.0.1:7890"

[channels.mochiclaw-weixin]
enabled = true
token = "your-channel-token"
extra = { base_url = "https://example.com" }

[models.gpt-4]
model = "gpt-4"
provider = "mochiclaw-openai"
api_base = "https://api.openai.com/v1"
api_key = "sk-..."

[models.deepseek-chat]
model = "deepseek-chat"
provider = "mochiclaw-openai"
api_base = "https://api.deepseek.com/v1"
api_key = "sk-..."
```

## `[agent]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `model` | string | `"gpt-4"` | Default model name (references `[models]`) |
| `max_iterations` | usize | `40` | Max tool-call iterations per message |
| `workspace` | string | `"."` | Workspace directory for sessions and files |

## `[runtime]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `plugin_dirs` | array | `[]` | Directories to search for plugins |

### `[runtime.log]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `level` | string | `"info"` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `dir` | string | `.logs` | Log directory |
| `max_age_days` | number | - | Auto-delete logs older than N days |

### `[runtime.network]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `use_system_proxy` | bool | `false` | Use system HTTP proxy |

## `[plugins]`

Per-plugin configuration. Key is the plugin name.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable/disable plugin |
| `proxy_url` | string | - | Per-plugin HTTP proxy override |

### `[plugins.PLUGIN.capabilities]`

Override plugin capabilities (merged with manifest).

| Field | Type | Description |
|-------|------|-------------|
| `network.enabled` | bool | Enable/disable network |
| `network.allowed_hosts` | array | Append to allowed hosts |
| `network.denied_hosts` | array | Append to denied hosts |
| `fs.enabled` | bool | Enable/disable filesystem |
| `fs.allowed_root` | string | Override allowed root |
| `fs.read_whitelist` | array | Append to read whitelist |
| `fs.write_whitelist` | array | Append to write whitelist |
| `fs.read_blacklist` | array | Append to read blacklist |
| `fs.write_blacklist` | array | Append to write blacklist |
| `allowed_kv_read` | array | Append to allowed KV read list |

## `[channels]`

Channel plugin configuration.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable/disable channel |
| `token` | string | - | Channel-specific token |
| `extra` | table | `{}` | Channel-specific extra config |

## `[models]`

Model configurations. Key is the model name.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `model` | string | Yes | Model identifier for provider |
| `provider` | string | Yes | Provider plugin name |
| `api_base` | string | No | API base URL override |
| `api_key` | string | No | API key (or use env var) |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `HTTP_PROXY` | Global HTTP proxy (fallback when `proxy_url` not set) |
| `api_key` | Can be used instead of `api_key` in config |

## Capability Merge Rules

User config capabilities are merged with manifest capabilities:

- **Booleans** (`enabled`): User config takes precedence
- **Strings** (`allowed_root`): User config takes precedence
- **Lists** (whitelists, blacklists): User items are **appended** to manifest
