# Contributing to Mochiclaw

[简体中文](../zh/contributing.md) | English

---

## Getting Started

Mochiclaw is a lambda-based AI Agent runtime framework built with Rust and Extism WASM. Contributions are welcome!

### Development Environment

```bash
# Install Rust (wasm32 target required)
rustup target add wasm32-unknown-unknown

# Install just (build tool)
cargo install just

# Clone and build
git clone https://github.com/your-fork/mochiclaw.git
cd mochiclaw
just build
```

## Development Workflow

### Building

```bash
# Build everything (native + lambdas)
just build

# Build native only
just build-native

# Build lambdas only
just build-lambda

# Build specific lambda
cargo build --release --target wasm32-unknown-unknown -p mochi-openai
```

### Code Quality

```bash
# Format code
just fmt

# Lint code
just lint

# Run tests
just test
```

## Project Structure

| Crate | Purpose |
|-------|---------|
| `mochiclaw-cli` | CLI entry point |
| `mochiclaw-core` | Agent runtime (AgentLoop, MessageBus, SessionManager) |
| `mochiclaw-lambda` | WASM lambda host (LambdaHost, Pool) |
| `mochiclaw-sdk` | Shared types for host and lambdas |
| `mochiclaw-config` | Configuration structures |

Official lambdas under `lambdas/`:
- `mochi-openai` - OpenAI-compatible LLM provider
- `mochi-fs` - Filesystem tools
- `mochi-weixin` - WeChat channel

## Writing Lambdas

See [lambda-tutorial.md](./lambda-tutorial.md) for detailed lambda development guide.

## Commit Guidelines

```
feat: add new lambda type
fix: resolve agent loop deadlock
docs: add configuration guide
refactor: simplify message bus
test: add integration tests for fs lambda
```

## Reporting Issues

- Use GitHub Issues
- Include: Rust version, platform, Extism version
- Provide minimal reproduction case

## Code of Conduct

- Be respectful and inclusive
- Focus on technical discussions
- Follow Rust community guidelines
