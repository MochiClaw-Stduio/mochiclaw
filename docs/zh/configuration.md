# 配置参考

English | [简体中文](../zh/configuration.md)

---

## 概述

Mochiclaw 使用 `config.toml` 进行配置。本文档描述所有可用选项。

## 完整示例

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

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `model` | string | `"gpt-4"` | 默认模型名（引用 `[models]`） |
| `max_iterations` | usize | `40` | 每条消息的最大 tool-call 迭代次数 |
| `workspace` | string | `"."` | 工作目录（sessions 和文件） |

## `[runtime]`

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `plugin_dirs` | array | `[]` | 搜索插件的目录 |

### `[runtime.log]`

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `level` | string | `"info"` | 日志级别：`trace`, `debug`, `info`, `warn`, `error` |
| `dir` | string | `.logs` | 日志目录 |
| `max_age_days` | number | - | 自动删除 N 天前的日志 |

### `[runtime.network]`

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `use_system_proxy` | bool | `false` | 使用系统 HTTP 代理 |

## `[plugins]`

每个插件的配置。Key 是插件名。

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `enabled` | bool | `true` | 启用/禁用插件 |
| `proxy_url` | string | - | 每个插件的 HTTP 代理覆盖 |

### `[plugins.PLUGIN.capabilities]`

覆盖插件能力（与 manifest 合并）。

| 字段 | 类型 | 描述 |
|------|------|------|
| `network.enabled` | bool | 启用/禁用网络 |
| `network.allowed_hosts` | array | 追加到允许的主机列表 |
| `network.denied_hosts` | array | 追加到拒绝的主机列表 |
| `fs.enabled` | bool | 启用/禁用文件系统 |
| `fs.allowed_root` | string | 覆盖允许的根目录 |
| `fs.read_whitelist` | array | 追加到读白名单 |
| `fs.write_whitelist` | array | 追加到写白名单 |
| `fs.read_blacklist` | array | 追加到读黑名单 |
| `fs.write_blacklist` | array | 追加到写黑名单 |
| `allowed_kv_read` | array | 追加到允许的 KV 读列表 |

## `[channels]`

Channel 插件配置。

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `enabled` | bool | `true` | 启用/禁用 channel |
| `token` | string | - | Channel 特定 token |
| `extra` | table | `{}` | Channel 特定额外配置 |

## `[models]`

模型配置。Key 是模型名。

| 字段 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `model` | string | 是 | Provider 的模型标识符 |
| `provider` | string | 是 | Provider 插件名 |
| `api_base` | string | 否 | API base URL 覆盖 |
| `api_key` | string | 否 | API key（或使用环境变量） |

## 环境变量

| 变量 | 描述 |
|------|------|
| `HTTP_PROXY` | 全局 HTTP 代理（当 `proxy_url` 未设置时的后备） |
| `api_key` | 可替代 config 中的 `api_key` |

## 能力合并规则

用户配置能力与 manifest 能力合并：

- **布尔值** (`enabled`): 用户配置优先
- **字符串** (`allowed_root`): 用户配置优先
- **列表** (白名单、黑名单): 用户项**追加**到 manifest
