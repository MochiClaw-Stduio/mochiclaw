# 架构设计

English | [简体中文](../zh/architecture.md)

---

## 概述

Mochiclaw 是一个基于插件的 AI Agent 运行时。架构分为四个主要层次：

```
┌─────────────────────────────────────────┐
│              mochiclaw-cli              │  入口
├─────────────────────────────────────────┤
│             mochiclaw-core              │  Agent 编排
│  ┌──────────┐ ┌──────────┐ ┌─────────┐  │
│  │AgentLoop │ │MessageBus│ │Session  │  │
│  └──────────┘ └──────────┘ └─────────┘  │
├─────────────────────────────────────────┤
│            mochiclaw-plugin             │  插件主机 (Extism)
│  ┌──────────────────────────────────┐   │
│  │  PluginHost  │  Pool  │ Host Fn  │   │
│  └──────────────────────────────────┘   │
├─────────────────────────────────────────┤
│              plugins/                   │  WASM 插件
│  ┌─────────┐ ┌─────────┐ ┌─────────┐    │
│  │ OpenAI  │ │   FS    │ │ WeChat  │    │
│  │Provider │ │  Tool   │ │ Channel │    │
│  └─────────┘ └─────────┘ └─────────┘    │
└─────────────────────────────────────────┘
```

## 核心组件

### AgentLoop (`mochiclaw-core`)

`AgentLoop` 是核心编排器：

1. **Poll** - 查询 channel 插件获取新消息
2. **Route** - 将消息路由到相应处理器
3. **Execute** - 运行 LLM + tools 的 agent 迭代
4. **Respond** - 通过 channel 插件发送响应

```
消息 → AgentLoop → [Session] → LLM Provider → [Tools] → 响应
```

关键字段：
- `bus: MessageBus` - 内部消息路由
- `sessions: SessionManager` - 对话历史
- `commands: CommandRegistry` - 斜杠命令处理
- `tool_definitions` - 插件提供的可用工具

### MessageBus (`mochiclaw-core`)

多生产者单消费者（mpsc）消息总线，用于内部通信。

### SessionManager (`mochiclaw-core`)

管理存储在磁盘上的对话历史。每个会话在 `sessions/` 下有独立文件。

### PluginHost (`mochiclaw-plugin`)

管理 WASM 插件生命周期：

| 组件 | 用途 |
|------|------|
| `CompiledPlugin` | JIT 编译的 WASM 模块（每个插件一个） |
| `Pool` | 用于并发的运行实例池 |
| `HostFunctions` | 暴露给插件的能力 |

### 插件类型

| 类型 | 接口 | 示例 |
|------|------|------|
| `provider` | `chat`, `chat_stream` | mochi-openai |
| `channel` | `poll`, `send_text`, `set_typing` | mochi-weixin |
| `tool` | `execute_tool`, `get_tools` | mochi-fs |
| `command` | `execute` | - |

## 数据流

### 消息处理

```
1. Channel Plugin (poll) → InboundMessage
2. AgentLoop → SessionManager (追加到历史)
3. AgentLoop → ContextBuilder (构建系统提示)
4. AgentLoop → Provider Plugin (chat request)
5. 如果有 tool_calls:
   a. AgentLoop → Tool Plugin (execute_tool)
   b. 重复 4-5 直到没有 tool_calls
6. AgentLoop → Channel Plugin (send_text response)
```

### 能力 enforcement

```
Plugin Manifest → Extism Manifest
                     ↓
              Allowed Hosts
                     ↓
              PluginHost host_http_request()
                     ↓
              检查 whitelist/blacklist
                     ↓
              执行或拒绝
```

## 关键文件

| 文件 | 描述 |
|------|------|
| `core/src/agent.rs` | AgentLoop 实现 |
| `core/src/bus.rs` | MessageBus |
| `core/src/session.rs` | SessionManager |
| `plugin/src/plugin.rs` | PluginHost |
| `plugin/src/host/mod.rs` | Host functions |
| `sdk/src/lib.rs` | 共享类型 |
