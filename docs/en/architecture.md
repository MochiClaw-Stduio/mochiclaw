# Architecture

[简体中文](../zh/architecture.md) | English

---

## Overview

Mochiclaw is an async-first AI Agent runtime built on a lambda architecture. The core design principle: **async tasks are cheap, Wasm threads are expensive**. Hundreds of channel polling tasks can run simultaneously in async space with minimal memory, while actual Wasm execution happens only when needed via thread pools.

## Core Design Principle

| Resource | Cost | Example |
|----------|------|---------|
| Async Task (Tokio) | **Extremely cheap** | 1000 channel pollers with minimal memory |
| Wasm Thread | **Expensive** | Pool-based execution, only during actual calls |

Channel polling tasks spend most of their time `await`ing network responses—consuming no OS threads. Wasm resources are only consumed during the brief moments when `spawn_blocking` executes actual lambda code.

## Architecture Layers

```
┌───────────────────────────────────────────────────────────────┐
│                        mochiclaw-cli                          │  Entry Point
├───────────────────────────────────────────────────────────────┤
│                       mochiclaw-core                          │  Agent Orchestration
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐ │
│  │  AgentLoop   │  │  MessageBus  │  │  SessionManager      │ │
│  └──────────────┘  └──────────────┘  └──────────────────────┘ │
│                                                               │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  Poller (100s of independent async tasks)              │   │
│  │  Each channel → tokio::spawn → await poll → feed bus   │   │
│  └────────────────────────────────────────────────────────┘   │
├───────────────────────────────────────────────────────────────┤
│                      mochiclaw-lambda                         │  Lambda Runtime
│  ┌───────────────────────────────────────────────────────┐    │
│  │  LambdaHost (Pool-based concurrent execution)         │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐               │    │
│  │  │ Pool     │ │ Pool     │ │ Pool     │  (per lambda) │    │
│  │  │ (openai) │ │ (fs)     │ │ (weixin) │               │    │
│  │  └──────────┘ └──────────┘ └──────────┘               │    │
│  └───────────────────────────────────────────────────────┘    │
│  ┌───────────────────────────────────────────────────────┐    │
│  │  AsyncHttpExecutor (host-side HTTP effect execution)  │    │
│  └───────────────────────────────────────────────────────┘    │
├───────────────────────────────────────────────────────────────┤
│                        lambdas/                               │  WASM Lambdas
│  ┌─────────┐ ┌─────────┐ ┌─────────┐                          │
│  │ OpenAI  │ │   FS    │ │ WeChat  │                          │
│  │Provider │ │  Tool   │ │ Channel │                          │
│  └─────────┘ └─────────┘ └─────────┘                          │
└───────────────────────────────────────────────────────────────┘
```

## Core Components

### AgentLoop (`mochiclaw-core`)

The `AgentLoop` is the central orchestrator:

1. **Spawn Pollers** - Launch independent async tasks for each channel
2. **Consume** - Receive `InboundMessage` from MessageBus
3. **Execute** - Run agent iterations with LLM + tools via `lambda_loop`
4. **Respond** - Send responses back via channel lambdas

Key fields:
- `bus: MessageBus` - Internal message routing (mpsc)
- `sessions: SessionManager` - Conversation history
- `http_executor: AsyncHttpExecutor` - Executes HTTP effects from lambdas
- `poll_state` - Per-channel state for resumable polling

### MessageBus (`mochiclaw-core`)

Multi-producer single-consumer (mpsc) message bus. Poll tasks produce, AgentLoop consumes.

### Poller (`mochiclaw-core`)

Each channel gets an independent `tokio::spawn` task:

```rust
spawn_channel_poller(channel_name, config, bus, lambda_host, http_executor, poll_state)
```

- Runs infinite loop with 2-second intervals
- Calls `lambda_call_typed(PreparePoll)` on each tick
- Feeds resulting messages to MessageBus
- Persists poll state (e.g., sync cursor) across iterations

### LambdaHost (`mochiclaw-lambda`)

Manages WASM lambda lifecycle with Pool-based concurrency:

| Component | Purpose |
|-----------|---------|
| `CompiledPlugin` | JIT-compiled WASM module (shared across pool) |
| `Pool` | Factory for concurrent `Plugin` instances (max = CPU cores) |
| `HostFunctions` | Capabilities: FS, KV, Rand (HTTP moved to host layer) |
| `LambdaContext` | Merged manifest + config per lambda |

### AsyncHttpExecutor (`mochiclaw-core`)

Executes `HttpEffect` returned by lambdas **on the host side**:

- **Permission enforcement**: Checks `allowed_hosts`/`denied_hosts` before execution
- **Proxy support**: Per-lambda proxy URL or system proxy
- **Parallel execution**: `futures::join_all()` for multiple effects

### LambdaLoop (`mochiclaw-core`)

Unified lambda calling engine with replay mechanism:

```rust
// Two scenarios based on LambdaOutput { effect, step_id, new_history }
lambda_call(lambda_host, http_executor, history_store, execution_id, lambda_name, action, payload)
```

| LambdaOutput | Behavior |
|--------------|----------|
| `Finished(result)` | Task complete, clear history, return result |
| `Suspended { effect, step_id, new_history }` | Execute effect, merge history, continue loop |

## Lambda Types & Actions

All lambdas use a unified `lambda_function` entry point with `Action` dispatch:

| Lambda Type | Actions |
|-------------|---------|
| `provider` | `Chat` |
| `channel` | `PreparePoll`, `FormatSend`, `SetTyping`, `Login`, `CheckLogin` |
| `tool` | `GetTools`, `ExecuteTool` |

### Unified LambdaInput/LambdaOutput

```rust
struct LambdaInput {
    version: u32,
    action: Action,
    payload: Vec<u8>,         // Action params (MessagePack)
    history: HashMap<String, Vec<u8>>,  // Completed steps history (for replay)
}

enum LambdaOutput {
    Finished(Vec<u8>),        // Task complete, return final result
    Suspended {
        effect: Effect,       // Effect to execute
        step_id: String,      // Step unique identifier
        new_history: HashMap<String, Vec<u8>>,  // New history from this run
    },
}
```

## Data Flow

### Message Processing

```
┌─ Channel Pollers (independent async tasks) ──────────────────────┐
│                                                                  │
│  [weixin] ──► lambda_call(PreparePoll) ──► messages ──┐          │
│                                                       │          │
│  [telegram] ─► lambda_call(PreparePoll) ──► messages ─┼──► Bus   │
│                                                       │          │
│  [slack] ───► lambda_call(PreparePoll) ──► messages ──┘          │
└──────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─ AgentLoop ──────────────────────────────────────────────────────┐
│                                                                  │
│  recv_inbound() ─► Session ─► ContextBuilder                     │
│                                    │                             │
│                          lambda_call(Chat) ──► LLM               │
│                                    │                             │
│                          If tool_calls:                          │
│                           lambda_call(ExecuteTool)               │
│                                    │                             │
│                          lambda_call(FormatSend) ──► Bus         │
└──────────────────────────────────────────────────────────────────┘
```

### HTTP Effect Execution (Replay Mechanism)

```
Lambda (with Context)              Host                            External
      │                              │                                │
      │ lambda_call(Chat)            │                                │
      │◄─────────────────────────────┤                                │
      │                              │                                │
      │ ctx.http() -> Suspend        │                                │
      │ LambdaOutput::Suspended {    │                                │
      │   effect: HttpEffect,        │                                │
      │   step_id: "chat_1",         │                                │
      │   new_history: {}            │                                │
      │ }                            │                                │
      │─────────────────────────────►│                                │
      │                              │                                │
      │                    AsyncHttpExecutor                          │
      │                    .execute_effect()                          │
      │                              │                                │
      │                    HTTP Request ─────────────────────────────►│
      │                              │                                │
      │                    EffectResult { success, response }         │
      │◄─────────────────────────────│                                │
      │                              │                                │
      │                    history_store.merge(step_id, result)        │
      │                              │                                │
      │ lambda_call(Chat)            │                                │
      │   (with history)             │                                │
      │◄─────────────────────────────┤                                │
      │                              │                                │
      │ ctx.http() -> cached result  │                                │
      │ LambdaOutput::Finished {     │                                │
      │   result: ChatResponse       │                                │
      │ }                            │                                │
      │─────────────────────────────►│ (return to AgentLoop)          │
```

**Replay mechanism explanation**:
1. When lambda calls `ctx.http()`, check `history` for cached result
2. If not cached, throw `SuspendSignal`, lambda returns `Suspended`
3. Host executes HTTP, stores result in `history_store`
4. On next call, `ctx.http()` returns cached result directly from history

## Capability Enforcement

```
Lambda Manifest + Config
         │
         ▼
┌─────────────────────────┐
│   Merged Capabilities   │
│   (allowed_hosts,       │
│    fs permissions, etc) │
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────────────────────┐
│          AsyncHttpExecutor              │
│                                         │
│  1. Check network_enabled()             │
│  2. Check is_host_allowed(url)          │
│     (glob patterns, blacklist wins)     │
│  3. Apply proxy settings                │
│  4. Execute HTTP                        │
│  5. Return EffectResult                 │
└─────────────────────────────────────────┘
```

## Key Files

| File | Description |
|------|-------------|
| `core/src/agent_loop.rs` | AgentLoop implementation |
| `core/src/lambda_loop.rs` | Unified lambda calling engine with effect loop |
| `core/src/http_executor.rs` | Async HTTP effect executor |
| `core/src/poller.rs` | Channel polling task spawner |
| `core/src/bus.rs` | MessageBus |
| `core/src/session.rs` | SessionManager |
| `lambda/src/lambda.rs` | LambdaHost with Pool mechanism |
| `lambda/src/context.rs` | LambdaContext (manifest + config) |
| `lambda/src/host/mod.rs` | Host functions (FS, KV, Rand) |
| `sdk/src/lambda/mod.rs` | Action, LambdaInput, LambdaOutput types |
