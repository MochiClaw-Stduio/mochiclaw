# Mochiclaw

A lambda-based AI Agent runtime framework with WASM lambda isolation.

[简体中文](./README.md) | English

---

## Features

- **Lambda Architecture**: Extend functionality via WASM lambdas with hot-swapping support
- **Isolated Execution**: Each lambda runs in an independent WASM VM for security
- **Flexible Capability Control**: Fine-grained network and filesystem access control
- **Multi-Lambda Support**: Load multiple lambdas simultaneously with concurrent execution via Pool
- **Effect System**: HTTP requests declared as Effects, executed by host with permission enforcement
- **Provider Lambdas**: Custom LLM Provider support (e.g., OpenAI-compatible API)
- **Channel Lambdas**: Custom message channel support (e.g., WeChat)
- **Tool Lambdas**: Filesystem and more tools

## Project Structure

```
mochiclaw/
├── cli          # CLI main program
├── config       # Configuration management
├── core         # Core runtime
│   ├── agent_loop.rs    # Agent orchestrator
│   ├── lambda_loop.rs   # Lambda calling engine (with Effect loop)
│   ├── http_executor.rs # HTTP Effect executor
│   └── poller.rs       # Channel polling tasks
├── lambda       # Lambda runtime management
├── sdk          # Lambda SDK
├── lambdas/               # Official lambdas
│   ├── openai   # OpenAI-compatible Provider
│   ├── fs       # Filesystem tools
│   └── weixin   # WeChat Channel
└── workspace/             # Working directory
```

## Documentation

For detailed documentation, see [docs/en/README.md](./docs/en/README.md).


## Quick Start

### Build

```bash
# Build everything (native + lambdas)
just build

# Build native only
just build-native

# Build lambdas only
just build-lambda
```

### Configure

Edit `config.toml`:

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

### Run

```bash
cargo run --release -p mochiclaw-cli
```

## Official Lambdas

| Lambda | Type | Description |
|--------|------|-------------|
| mochi-openai | Provider | OpenAI-compatible API |
| mochi-fs | Tool | Filesystem operations |
| mochi-weixin | Channel | WeChat message channel |

## Development

```bash
# Format code
just fmt

# Lint code
just lint

# Run tests
just test
```

## License

MIT
