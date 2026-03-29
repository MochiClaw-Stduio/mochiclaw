# Mochiclaw

一个基于 Lambda 的 AI Agent 运行时框架，支持 WASM Lambda 隔离执行。

[English](./README_en.md) | 简体中文

---

## 特性

- **Lambda 架构**：通过 WASM Lambda 扩展功能，支持热插拔
- **隔离执行**：每个 Lambda 在独立的 WASM 虚拟机中运行，确保安全
- **灵活的能力控制**：细粒度的网络、文件系统访问控制
- **多 Lambda 支持**：支持同时加载多个 Lambda，通过 Pool 实现并发执行
- **Effect 系统**：HTTP 请求声明为 Effects，由主机统一执行
- **Provider Lambda**：支持自定义 LLM Provider（如 OpenAI 兼容接口）
- **Channel Lambda**：支持自定义消息通道（如微信）
- **Tool Lambda**：提供文件系统等工具

## 项目结构

```
mochiclaw/
├── cli          # CLI 主程序
├── config       # 配置管理
├── core         # 核心运行时
│   ├── agent_loop.rs    # Agent 编排器
│   ├── lambda_loop.rs   # Lambda 调用引擎（含 Effect 循环）
│   ├── http_executor.rs # HTTP Effect 执行器
│   └── poller.rs       # 通道轮询任务
├── lambda       # Lambda 运行时管理
├── sdk          # Lambda SDK
├── lambdas/               # 官方 Lambda
│   ├── openai   # OpenAI 兼容 Provider
│   ├── fs       # 文件系统工具
│   └── weixin   # 微信 Channel
└── workspace/             # 工作目录
```

## 文档

详细文档请查看 [docs/zh/README.md](./docs/zh/README.md)。

## 快速开始

### 构建

```bash
# 构建所有（原生 + Lambda）
just build

# 仅构建原生程序
just build-native

# 仅构建 Lambda
just build-lambda
```

### 配置

编辑 `config.toml`:

```toml
[agent]
model = "deepseek-chat"
max_iterations = 40
workspace = "./workspace"

[runtime]
lambda_dirs = ["./target/lambdas"]

[models.deepseek-chat]
model = "deepseek-chat"
provider = "mochi-openai"
api_base = "https://api.deepseek.com/v1"
api_key = "sk-your-key"
```

### 运行

```bash
cargo run --release -p mochiclaw-cli
```

## 官方 Lambda

| Lambda | 类型 | 说明 |
|--------|------|------|
| mochi-openai | Provider | OpenAI 兼容 API |
| mochi-fs | Tool | 文件系统操作 |
| mochi-weixin | Channel | 微信消息通道 |

## 开发

```bash
# 代码格式化
just fmt

# 代码检查
just lint

# 运行测试
just test
```

## 许可证

MIT
