# Architecture

[简体中文](../zh/architecture.md) | English

---

## Overview

Mochiclaw is a plugin-based AI Agent runtime. The architecture consists of four main layers:

```
┌─────────────────────────────────────────┐
│              mochiclaw-cli              │  Entry Point
├─────────────────────────────────────────┤
│             mochiclaw-core              │  Agent Orchestration
│  ┌──────────┐ ┌──────────┐ ┌─────────┐  │
│  │AgentLoop │ │MessageBus│ │Session  │  │
│  └──────────┘ └──────────┘ └─────────┘  │
├─────────────────────────────────────────┤
│            mochiclaw-plugin             │  Plugin Host (Extism)
│  ┌──────────────────────────────────┐   │
│  │  PluginHost  │  Pool  │ Host Fn  │   │
│  └──────────────────────────────────┘   │
├─────────────────────────────────────────┤
│              plugins/                   │  WASM Plugins
│  ┌─────────┐ ┌─────────┐ ┌─────────┐    │
│  │ OpenAI  │ │   FS    │ │ WeChat  │    │
│  │Provider │ │  Tool   │ │ Channel │    │
│  └─────────┘ └─────────┘ └─────────┘    │
└─────────────────────────────────────────┘
```

## Core Components

### AgentLoop (`mochiclaw-core`)

The `AgentLoop` is the central orchestrator:

1. **Poll** - Query channel plugins for new messages
2. **Route** - Direct messages to appropriate handlers
3. **Execute** - Run agent iterations with LLM + tools
4. **Respond** - Send responses back via channel plugins

```
Message → AgentLoop → [Session] → LLM Provider → [Tools] → Response
```

Key fields:
- `bus: MessageBus` - Internal message routing
- `sessions: SessionManager` - Conversation history
- `commands: CommandRegistry` - Slash command handling
- `tool_definitions` - Available tools from plugins

### MessageBus (`mochiclaw-core`)

Multi-producer single-consumer (mpsc) message bus for internal communication.

### SessionManager (`mochiclaw-core`)

Manages conversation history stored on disk. Each session has its own file under `sessions/`.

### PluginHost (`mochiclaw-plugin`)

Manages WASM plugin lifecycle:

| Component | Purpose |
|-----------|---------|
| `CompiledPlugin` | JIT-compiled WASM module (one per plugin) |
| `Pool` | Pool of running instances for concurrency |
| `HostFunctions` | Capabilities exposed to plugins |

### Plugin Types

| Type | Interface | Example |
|------|-----------|---------|
| `provider` | `chat`, `chat_stream` | mochiclaw-openai |
| `channel` | `poll`, `send_text`, `set_typing` | mochiclaw-weixin |
| `tool` | `execute_tool`, `get_tools` | mochiclaw-fs |
| `command` | `execute` | - |

## Data Flow

### Message Processing

```
1. Channel Plugin (poll) → InboundMessage
2. AgentLoop → SessionManager (append to history)
3. AgentLoop → ContextBuilder (build system prompt)
4. AgentLoop → Provider Plugin (chat request)
5. If tool_calls:
   a. AgentLoop → Tool Plugin (execute_tool)
   b. Repeat 4-5 until no tool_calls
6. AgentLoop → Channel Plugin (send_text response)
```

### Capability Enforcement

```
Plugin Manifest → Extism Manifest
                     ↓
              Allowed Hosts
                     ↓
              PluginHost host_http_request()
                     ↓
              Check against whitelist/blacklist
                     ↓
              Execute or Deny
```

## Key Files

| File | Description |
|------|-------------|
| `mochiclaw-core/src/agent.rs` | AgentLoop implementation |
| `mochiclaw-core/src/bus.rs` | MessageBus |
| `mochiclaw-core/src/session.rs` | SessionManager |
| `mochiclaw-plugin/src/plugin.rs` | PluginHost |
| `mochiclaw-plugin/src/host/mod.rs` | Host functions |
| `mochiclaw-sdk/src/lib.rs` | Shared types |
