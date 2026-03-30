# 架构设计

English | [简体中文](../zh/architecture.md)

---

## 概述

Mochiclaw 是一个异步优先的 AI Agent 运行时，建立在 lambda 架构之上。核心设计原则：**异步 Task 极其廉价，Wasm 线程非常昂贵**。数百个通道轮询任务可以在异步空间中同时运行，内存占用极小，而实际 Wasm 执行只在需要时通过线程池进行。

## 核心设计原则

| 资源 | 成本 | 示例 |
|------|------|------|
| Async Task (Tokio) | **极其廉价** | 1000 个通道轮询器，最小内存占用 |
| Wasm 线程 | **昂贵** | 基于 Pool 执行，仅在实际调用期间占用 |

通道轮询任务大部分时间在 `await` 网络响应——不占用任何 OS 线程。Wasm 资源仅在 `spawn_blocking` 执行实际 lambda 代码的短暂时刻消耗。

## 架构层次

```
┌───────────────────────────────────────────────────────────────┐
│                        mochiclaw-cli                          │  入口
├───────────────────────────────────────────────────────────────┤
│                       mochiclaw-core                          │  Agent 编排
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐ │
│  │  AgentLoop   │  │  MessageBus  │  │  SessionManager      │ │
│  └──────────────┘  └──────────────┘  └──────────────────────┘ │
│                                                               │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  Poller (数百个独立的异步任务)                         │   │
│  │  每个通道 → tokio::spawn → await poll → feed bus       │   │
│  └────────────────────────────────────────────────────────┘   │
├───────────────────────────────────────────────────────────────┤
│                      mochiclaw-lambda                         │  Lambda 运行时
│  ┌───────────────────────────────────────────────────────┐    │
│  │  LambdaHost (基于 Pool 的并发执行)                    │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐               │    │
│  │  │ Pool     │ │ Pool     │ │ Pool     │  (per lambda) │    │
│  │  │ (openai) │ │ (fs)     │ │ (weixin) │               │    │
│  │  └──────────┘ └──────────┘ └──────────┘               │    │
│  └───────────────────────────────────────────────────────┘    │
│  ┌───────────────────────────────────────────────────────┐    │
│  │  AsyncHttpExecutor (主机端 HTTP effect 执行)          │    │
│  └───────────────────────────────────────────────────────┘    │
├───────────────────────────────────────────────────────────────┤
│                        lambdas/                               │  WASM Lambdas
│  ┌─────────┐ ┌─────────┐ ┌─────────┐                          │
│  │ OpenAI  │ │   FS    │ │ WeChat  │                          │
│  │Provider │ │  Tool   │ │ Channel │                          │
│  └─────────┘ └─────────┘ └─────────┘                          │
└───────────────────────────────────────────────────────────────┘
```

## 核心组件

### AgentLoop (`mochiclaw-core`)

`AgentLoop` 是核心编排器：

1. **Spawn Pollers** - 为每个通道启动独立的异步任务
2. **Consume** - 从 MessageBus 接收 `InboundMessage`
3. **Execute** - 通过 `lambda_loop` 运行 LLM + tools 的 agent 迭代
4. **Respond** - 通过 channel lambda 发送响应

关键字段：
- `bus: MessageBus` - 内部消息路由（mpsc）
- `sessions: SessionManager` - 对话历史
- `http_executor: AsyncHttpExecutor` - 执行 lambda 返回的 HTTP effects
- `poll_state` - 每个通道的轮询状态，用于可恢复轮询

### MessageBus (`mochiclaw-core`)

多生产者单消费者（mpsc）消息总线。轮询任务生产，AgentLoop 消费。

### Poller (`mochiclaw-core`)

每个通道有一个独立的 `tokio::spawn` 任务：

```rust
spawn_channel_poller(channel_name, config, bus, lambda_host, http_executor, poll_state)
```

- 以 2 秒间隔运行无限循环
- 每次 tick 调用 `lambda_call_typed(PreparePoll)`
- 将结果消息发送到 MessageBus
- 在迭代之间持久化轮询状态（如同步游标）

### LambdaHost (`mochiclaw-lambda`)

通过基于 Pool 的并发管理 WASM lambda 生命周期：

| 组件 | 用途 |
|------|------|
| `CompiledPlugin` | JIT 编译的 WASM 模块（跨 pool 共享） |
| `Pool` | 并发 `Plugin` 实例的工厂（最大 = CPU 核心数） |
| `HostFunctions` | 暴露的能力：FS、KV、Rand（HTTP 移至主机层） |
| `LambdaContext` | 每个 lambda 的合并 manifest + config |

### AsyncHttpExecutor (`mochiclaw-core`)

在**主机端**执行 lambda 返回的 `HttpEffect`：

- **权限 enforcement**：执行前检查 `allowed_hosts`/`denied_hosts`
- **代理支持**：每个 lambda 的 proxy URL 或系统代理
- **并行执行**：多个 effects 用 `futures::join_all()`

### LambdaLoop (`mochiclaw-core`)

统一的 lambda 调用引擎，带重放机制：

```rust
// 根据 LambdaOutput { effect, step_id, new_history } 的两种情况
lambda_call(lambda_host, http_executor, history_store, execution_id, lambda_name, action, payload)
```

| LambdaOutput | 行为 |
|--------------|------|
| `Finished(result)` | 执行完成，清理历史，返回结果 |
| `Suspended { effect, step_id, new_history }` | 执行 effect，合并历史，继续 loop |

## Lambda 类型与 Action

所有 lambda 使用统一的 `lambda_function` 入口点，通过 `Action` 分发：

| Lambda 类型 | Actions |
|-------------|---------|
| `provider` | `Chat` |
| `channel` | `PreparePoll`, `FormatSend`, `SetTyping`, `Login`, `CheckLogin` |
| `tool` | `GetTools`, `ExecuteTool` |

### 统一的 LambdaInput/LambdaOutput

```rust
struct LambdaInput {
    version: u32,
    action: Action,
    payload: Vec<u8>,        // Action 参数（MessagePack）
    history: HashMap<String, Vec<u8>>,  // 已完成步骤的历史（用于重放）
}

enum LambdaOutput {
    Finished(Vec<u8>),        // 任务完成，返回最终结果
    Suspended {
        effect: Effect,       // 需要执行的 Effect
        step_id: String,      // 步骤唯一标识
        new_history: HashMap<String, Vec<u8>>,  // 本次新产生的历史
    },
}
```

## 数据流

### 消息处理

```
┌─ 通道轮询器（独立的异步任务） ───────────────────────────────────┐
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
│                          如果有 tool_calls:                      │
│                           lambda_call(ExecuteTool)               │
│                                    │                             │
│                          lambda_call(FormatSend) ──► Bus         │
└──────────────────────────────────────────────────────────────────┘
```

### HTTP Effect 执行（重放机制）

```
Lambda (with Context)              Host                            外部
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

**重放机制说明**：
1. Lambda 调用 `ctx.http()` 时，先检查 `history` 是否有缓存
2. 若无缓存，抛出 `SuspendSignal`，lambda 返回 `Suspended`
3. 主机执行 HTTP，结果存入 `history_store`
4. 下次调用时，`ctx.http()` 直接从 history 返回缓存结果

## 能力 Enforcement

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

## 关键文件

| 文件 | 描述 |
|------|------|
| `core/src/agent_loop.rs` | AgentLoop 实现 |
| `core/src/lambda_loop.rs` | 统一的 lambda 调用引擎，带 effect 循环 |
| `core/src/http_executor.rs` | 异步 HTTP effect 执行器 |
| `core/src/poller.rs` | 通道轮询任务生成器 |
| `core/src/bus.rs` | MessageBus |
| `core/src/session.rs` | SessionManager |
| `lambda/src/lambda.rs` | LambdaHost 与 Pool 机制 |
| `lambda/src/context.rs` | LambdaContext（manifest + config） |
| `lambda/src/host/mod.rs` | Host functions（FS、KV、Rand） |
| `sdk/src/lambda/mod.rs` | Action、LambdaInput、LambdaOutput 类型 |
