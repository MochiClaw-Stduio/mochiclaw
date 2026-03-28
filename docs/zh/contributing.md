# 参与 Mochiclaw 开发

English | [简体中文](../zh/contributing.md)

---

## 入门

Mochiclaw 是一个基于 Rust 和 Extism WASM 的插件化 AI Agent 运行时框架。欢迎贡献代码！

### 开发环境

```bash
# 安装 Rust（需要 wasm32 target）
rustup target add wasm32-unknown-unknown

# 安装 just（构建工具）
cargo install just

# 克隆并构建
git clone https://github.com/your-fork/mochiclaw.git
cd mochiclaw
just build
```

## 开发流程

### 构建

```bash
# 构建所有（原生 + 插件）
just build

# 仅构建原生
just build-native

# 仅构建插件
just build-plugin

# 构建特定插件
cargo build --release --target wasm32-unknown-unknown -p mochiclaw-openai
```

### 代码质量

```bash
# 格式化代码
just fmt

# 代码检查
just lint

# 运行测试
just test
```

## 项目结构

| Crate | 职责 |
|-------|------|
| `mochiclaw-cli` | CLI 入口 |
| `mochiclaw-core` | Agent 运行时（AgentLoop、MessageBus、SessionManager） |
| `mochiclaw-plugin` | WASM 插件主机（PluginHost、Pool） |
| `mochiclaw-sdk` | 共享类型 |
| `mochiclaw-config` | 配置结构 |

`plugins/` 下的官方插件：
- `mochiclaw-openai` - OpenAI 兼容 LLM provider
- `mochiclaw-fs` - 文件系统工具
- `mochiclaw-weixin` - 微信 channel

## 编写插件

详细插件开发指南请参见 [plugin-tutorial.md](./plugin-tutorial.md)。

## 提交规范

```
feat: add new plugin type
fix: resolve agent loop deadlock
docs: add configuration guide
refactor: simplify message bus
test: add integration tests for fs plugin
```

## 报告问题

- 使用 GitHub Issues
- 包含：Rust 版本、平台、Extism 版本
- 提供最小复现案例

## 行为准则

- 尊重和包容
- 专注于技术讨论
- 遵循 Rust 社区规范
