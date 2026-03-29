# 斜杠命令

English | [简体中文](../zh/commands.md)

---

## 概述

斜杠命令以 `/` 开头，提供对 agent 功能的快速访问。它们从用户消息中解析，并在正常消息处理之前执行。

## 内置命令

### /help

显示可用命令及其描述。

```
/help
```

输出：
```
Available commands:
/help - Show available commands
/clear - Clear conversation history

Use /<command> to invoke a command.
```

### /clear

清除当前会话的对话历史。

```
/clear
```

参数（可选）：
- 会话 ID 或 "all" 清除所有会话

## 命令格式

命令从消息内容中解析：

| 格式 | 示例 | 解析结果 |
|------|------|---------|
| `/command` | `/help` | `("help", "")` |
| `/command args` | `/clear foo` | `("clear", "foo")` |

注意：
- 命令不区分大小写（`/HELP` = `/help`）
- 命令名称后的第一个空格用于分隔命令和参数
- 不以 `/` 开头的消息被视为普通消息

## 命令注册表

`CommandRegistry` 管理可用命令：

```rust
pub struct CommandRegistry {
    commands: HashMap<String, (CommandFn, CommandInfo)>,
}

// 命令函数签名
type CommandFn = Arc<dyn Fn(&str, &str, &str) -> Option<String> + Send + Sync>;
//                                ^channel  ^chat_id  ^args     ^response
```

### 注册

```rust
let mut registry = CommandRegistry::new();

registry.register("mycmd", "Does something special", |channel, chat_id, args| {
    Some(format!("Ran mycmd with args: {} in {} for {}", args, channel, chat_id))
});
```

### 执行

```rust
if let Some(response) = registry.execute("weixin", "user123", "mycmd", "arg1 arg2") {
    // 通过 channel 发送响应
}
```

## 插件命令

命令也可以由在 manifest 中声明 `features.command = true` 的插件提供：

```toml
[features]
command = true
```

插件命令通过 `LambdaHost::call` 调用 `execute` 函数。

## 解析

`parse_command` 函数从消息中提取命令：

```rust
use mochiclaw_core::commands::parse_command;

assert_eq!(parse_command("/help"), Some(("help".to_string(), "".to_string())));
assert_eq!(parse_command("/clear foo"), Some(("clear".to_string(), "foo".to_string())));
assert_eq!(parse_command("hello"), None);
```

## 流程

```
用户输入
    ↓
parse_command() → None? → 普通消息处理
    ↓
CommandRegistry.execute()
    ↓
内置处理器 / 插件调用
    ↓
通过 channel 发送响应
```
