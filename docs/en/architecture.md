# Architecture

[简体中文](../zh/architecture.md) | English

---

## Overview

Mochiclaw is a lambda-based AI Agent runtime. The architecture consists of four main layers:

```
┌─────────────────────────────────────────┐
│              mochiclaw-cli              │  Entry Point
├─────────────────────────────────────────┤
│             mochiclaw-core              │  Agent Orchestration
│  ┌──────────┐ ┌──────────┐ ┌─────────┐  │
│  │AgentLoop │ │MessageBus│ │Session  │  │
│  └──────────┘ └──────────┘ └─────────┘  │
├─────────────────────────────────────────┤
│            mochiclaw-lambda             │  Lambda Host (Extism)
│  ┌──────────────────────────────────┐   │
│  │  LambdaHost  │  Pool  │ Host Fn  │   │
│  └──────────────────────────────────┘   │
├─────────────────────────────────────────┤
│              lambdas/                   │  WASM Lambdas
│  ┌─────────┐ ┌─────────┐ ┌─────────┐    │
│  │ OpenAI  │ │   FS    │ │ WeChat  │    │
│  │Provider │ │  Tool   │ │ Channel │    │
│  └─────────┘ └─────────┘ └─────────┘    │
└─────────────────────────────────────────┘
```

## Core Components

### AgentLoop (`mochiclaw-core`)

The `AgentLoop` is the central orchestrator:

1. **Poll** - Query channel lambdas for new messages
2. **Route** - Direct messages to appropriate handlers
3. **Execute** - Run agent iterations with LLM + tools
4. **Respond** - Send responses back via channel lambdas

```
Message → AgentLoop → [Session] → LLM Provider → [Tools] → Response
```

Key fields:
- `bus: MessageBus` - Internal message routing
- `sessions: SessionManager` - Conversation history
- `commands: CommandRegistry` - Slash command handling
- `tool_definitions` - Available tools from lambdas

### MessageBus (`mochiclaw-core`)

Multi-producer single-consumer (mpsc) message bus for internal communication.

### SessionManager (`mochiclaw-core`)

Manages conversation history stored on disk. Each session has its own file under `sessions/`.

### LambdaHost (`mochiclaw-lambda`)

Manages WASM lambda lifecycle:

| Component | Purpose |
|-----------|---------|
| `CompiledPlugin` | JIT-compiled WASM module (one per lambda) |
| `Pool` | Pool of running instances for concurrency |
| `HostFunctions` | Capabilities exposed to lambdas |

### Lambda Types

| Type | Interface | Example |
|------|-----------|---------|
| `provider` | `chat`, `chat_stream` | mochi-openai |
| `channel` | `poll`, `send_text`, `set_typing` | mochi-weixin |
| `tool` | `execute_tool`, `get_tools` | mochi-fs |
| `command` | `execute` | - |

## Data Flow

### Message Processing

```
1. Channel Lambda (poll) → InboundMessage
2. AgentLoop → SessionManager (append to history)
3. AgentLoop → ContextBuilder (build system prompt)
4. AgentLoop → Provider Lambda (chat request)
5. If tool_calls:
   a. AgentLoop → Tool Lambda (execute_tool)
   b. Repeat 4-5 until no tool_calls
6. AgentLoop → Channel Lambda (send_text response)
```

### Capability Enforcement

```
Lambda Manifest → Extism Manifest
                     ↓
              Allowed Hosts
                     ↓
              LambdaHost host_http_request()
                     ↓
              Check against whitelist/blacklist
                     ↓
              Execute or Deny
```

## Key Files

| File | Description |
|------|-------------|
| `core/src/agent.rs` | AgentLoop implementation |
| `core/src/bus.rs` | MessageBus |
| `core/src/session.rs` | SessionManager |
| `lambda/src/lambda.rs` | LambdaHost |
| `lambda/src/host/mod.rs` | Host functions |
| `sdk/src/lib.rs` | Shared types |
