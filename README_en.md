# Mochiclaw

A plugin-based AI Agent runtime framework with WASM plugin isolation.

[简体中文](./README.md) | English

---

## Features

- **Plugin Architecture**: Extend functionality via WASM plugins with hot-swapping support
- **Isolated Execution**: Each plugin runs in an independent WASM VM for security
- **Flexible Capability Control**: Fine-grained network and filesystem access control
- **Multi-Plugin Support**: Load multiple plugins simultaneously with concurrent execution via Pool
- **Provider Plugins**: Custom LLM Provider support (e.g., OpenAI-compatible API)
- **Channel Plugins**: Custom message channel support (e.g., WeChat)
- **Tool Plugins**: Filesystem, HTTP requests, and more tools

## Project Structure

```
mochiclaw/
├── mochiclaw-cli          # CLI main program
├── mochiclaw-config       # Configuration management
├── mochiclaw-core         # Core runtime
├── mochiclaw-plugin       # Plugin management
├── mochiclaw-sdk          # Plugin SDK
├── plugins/               # Official plugins
│   ├── mochi-openai   # OpenAI-compatible Provider
│   ├── mochi-fs       # Filesystem tools
│   └── mochi-weixin   # WeChat Channel
└── workspace/             # Working directory
```

## Documentation

For detailed documentation, see [docs/en/README.md](./docs/en/README.md).


## Quick Start

### Build

```bash
# Build everything (native + plugins)
just build

# Build native only
just build-native

# Build plugins only
just build-plugin
```

### Configure

Edit `config.toml`:

```toml
[agent]
model = "deepseek-chat"
max_iterations = 40
workspace = "./workspace"

[runtime]
plugin_dirs = ["./target/plugins"]

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

## Official Plugins

| Plugin | Type | Description |
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
