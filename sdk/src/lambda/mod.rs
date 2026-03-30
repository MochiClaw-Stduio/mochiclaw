//! Lambda 统一入口 - 通用的插件调用接口
//!
//! Action 枚举和 LambdaInput/Output 统一定义在此，主机和插件共享

use extism_convert::{FromBytes, Msgpack, ToBytes};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Action 枚举 - 所有插件支持的动作
// ============================================================================

/// 所有插件支持的动作类型
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[serde(rename_all = "snake_case")]
#[encoding(Msgpack)]
pub enum Action {
    // Channel 动作（微信等）
    PreparePoll,
    FormatSend,
    SetTyping,

    // Provider 动作（OpenAI 等）
    Chat,

    // Login 动作（登录流程）
    Login,
    CheckLogin,

    // Tool 动作（工具执行）
    GetTools,
    ExecuteTool,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::PreparePoll => "prepare_poll",
            Action::FormatSend => "format_send",
            Action::SetTyping => "set_typing",
            Action::Chat => "chat",
            Action::Login => "login",
            Action::CheckLogin => "check_login",
            Action::GetTools => "get_tools",
            Action::ExecuteTool => "execute_tool",
        }
    }
}

// ============================================================================
// HttpEffect - 主机负责执行的 HTTP 指令
// ============================================================================

/// HTTP 请求指令，由主机异步执行
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct HttpEffect {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub timeout_ms: u32,
}

/// Effect 枚举，可以扩展其他类型
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub enum Effect {
    HttpRequest(HttpEffect),
}

/// 单个 Effect 的执行结果，用于回传给插件
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct EffectResult {
    /// 执行状态：true=成功，false=失败
    pub success: bool,
    /// 成功时返回的响应体（字符串）
    #[serde(default)]
    pub response: Option<String>,
    /// 失败时的错误信息
    #[serde(default)]
    pub error: Option<String>,
}

/// 多个 Effect 的执行结果列表
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct EffectResults {
    pub results: Vec<EffectResult>,
}

// ============================================================================
// LambdaInput / LambdaOutput - 插件入口参数和返回值（持久化执行模型）
// ============================================================================

/// 插件入口参数
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct LambdaInput {
    /// 版本号，用于未来兼容
    pub version: u32,
    /// 要执行的动作
    pub action: Action,
    /// MessagePack - action 对应的参数
    pub payload: Vec<u8>,
    /// 已完成的所有步骤结果（用于重放）
    #[serde(default)]
    pub history: HashMap<String, Vec<u8>>,
}

/// 插件返回值
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub enum LambdaOutput {
    /// 任务完全结束
    Finished(Vec<u8>),
    /// 任务挂起，需要执行 Effect，并带回本次运行新产生的增量历史
    Suspended {
        /// 需要执行的 Effect (boxed to reduce error size)
        effect: Box<Effect>,
        /// 步骤唯一标识
        step_id: String,
        /// 本次重放新产生的 step 结果，主机需将其合并到历史中
        new_history: HashMap<String, Vec<u8>>,
    },
}

// ============================================================================
// Context - 持久化执行上下文
// ============================================================================

/// 挂起信号，用于从 SDK 内部抛出
#[derive(Debug, Clone)]
pub struct SuspendSignal {
    /// 步骤唯一标识
    pub step_id: String,
    /// 需要执行的 Effect (boxed to reduce error size)
    pub effect: Box<Effect>,
}

impl SuspendSignal {
    pub fn new(step_id: &str, effect: Effect) -> Self {
        Self {
            step_id: step_id.to_string(),
            effect: Box::new(effect),
        }
    }
}

/// 持久化执行上下文
pub struct Context {
    /// 持久化历史（从数据库加载）
    history: HashMap<String, Vec<u8>>,
    /// 本次运行新产生的增量历史
    new_history: HashMap<String, Vec<u8>>,
    /// 等待执行的 effect（挂起时设置）
    pending_effect: Option<(String, Box<Effect>)>,
    /// 自动计数器：base_id -> 使用次数（解决循环中 ID 重复问题）
    counter: HashMap<String, u32>,
}

impl Context {
    /// 创建新的上下文
    pub fn new(history: HashMap<String, Vec<u8>>) -> Self {
        Self {
            history,
            new_history: HashMap::new(),
            pending_effect: None,
            counter: HashMap::new(),
        }
    }

    /// 确定性步骤：自动检查缓存，若无则执行并存入增量历史
    pub fn step<T, F>(&mut self, step_id: &str, func: F) -> Result<T, SuspendSignal>
    where
        T: for<'de> serde::Deserialize<'de> + serde::Serialize,
        F: FnOnce(&mut Context) -> Result<T, SuspendSignal>,
    {
        // 1. 检查旧历史
        if let Some(bytes) = self.history.get(step_id)
            && let Ok(result) = rmp_serde::from_slice(bytes)
        {
            return Ok(result);
        }
        // 2. 检查本次运行新产生的增量历史
        if let Some(bytes) = self.new_history.get(step_id)
            && let Ok(result) = rmp_serde::from_slice(bytes)
        {
            return Ok(result);
        }

        // 3. 执行真实逻辑
        let result = func(self)?;

        // 4. 序列化结果并存入增量历史
        let bytes = rmp_serde::to_vec(&result).map_err(|_| {
            SuspendSignal::new(
                step_id,
                Effect::HttpRequest(HttpEffect {
                    method: String::new(),
                    url: String::new(),
                    headers: HashMap::new(),
                    body: None,
                    timeout_ms: 0,
                }),
            )
        })?;
        self.new_history.insert(step_id.to_string(), bytes);

        Ok(result)
    }

    /// HTTP 请求，自动挂起/恢复
    /// 内部自动生成唯一 ID：`{base_id}_{count}`
    pub fn http(&mut self, base_id: &str, req: HttpEffect) -> Result<EffectResult, SuspendSignal> {
        // 自动计数，生成唯一 step_id
        let count = self.counter.entry(base_id.to_string()).or_insert(0);
        *count += 1;
        let step_id = format!("{}_{}", base_id, count);

        // 1. 检查旧历史
        if let Some(bytes) = self.history.get(&step_id)
            && let Ok(result) = rmp_serde::from_slice(bytes)
        {
            return Ok(result);
        }
        // 2. 检查本次运行新产生的增量历史
        if let Some(bytes) = self.new_history.get(&step_id)
            && let Ok(result) = rmp_serde::from_slice(bytes)
        {
            return Ok(result);
        }

        // 3. 需要执行 HTTP 请求 - 设置 pending_effect 并抛出中断信号
        self.pending_effect = Some((step_id.clone(), Box::new(Effect::HttpRequest(req.clone()))));
        Err(SuspendSignal::new(&step_id, Effect::HttpRequest(req)))
    }

    /// 获取待执行的 effect（由 #[mochi_main] 宏调用）
    pub fn take_pending_effect(&mut self) -> Option<(String, Box<Effect>)> {
        self.pending_effect.take()
    }

    /// 获取本次运行新产生的增量历史
    pub fn into_new_history(self) -> HashMap<String, Vec<u8>> {
        self.new_history
    }
}

// ============================================================================
// Channel 动作的 Payload 类型
// ============================================================================

/// prepare_poll 的输入参数
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct PreparePollInput {
    pub token: String,
}

/// prepare_poll 的输出结果
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct PreparePollOutput {
    pub synced: bool,
}

/// digest_response 的输出结果
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct DigestOutput {
    pub messages: Vec<super::message::InboundMessage>,
}

/// format_send 的输入参数
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct SendInput {
    pub token: String,
    pub to_user_id: String,
    pub content: String,
}

/// format_send 的输出结果
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct SendOutput {
    pub success: bool,
}

/// set_typing 的输入参数
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct SetTypingInput {
    pub token: String,
    pub chat_id: String,
    pub typing: bool, // true = start, false = stop
}

// ============================================================================
// Provider 动作的 Payload 类型
// ============================================================================

/// chat 的输入参数
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct ChatInput {
    pub request: super::provider::ChatRequest,
}

/// digest_chat 的输入参数
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct DigestChatInput {
    pub raw_response: String,
}

// ============================================================================
// Login 动作的 Payload 类型
// ============================================================================

/// login 的输入参数（包含配置）
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct LoginInput {
    /// Lambda-specific config bytes (e.g., previously saved token)
    pub config: Vec<u8>,
}

/// login 的输出结果
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct LoginOutput {
    /// Status: "logged_in", "need_qr", "error"
    pub status: String,
    /// QR code URL to display if status is "need_qr"
    #[serde(default)]
    pub qr_url: Option<String>,
    /// Temporary token for polling QR scan status if status is "need_qr"
    #[serde(default)]
    pub temp_token: Option<String>,
    /// Auth token if already logged in or after successful login
    #[serde(default)]
    pub token: Option<String>,
    /// Base URL for API calls
    #[serde(default)]
    pub base_url: Option<String>,
    /// Error message if status is "error"
    #[serde(default)]
    pub error: Option<String>,
}

/// check_login 的输入参数
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct CheckLoginInput {
    /// Temporary token from login response
    pub temp_token: String,
}

/// check_login 的输出结果
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct CheckLoginOutput {
    /// Status: "confirmed", "scaned", "expired", "error", or continue polling
    pub status: String,
    /// Auth token if status is "confirmed"
    #[serde(default)]
    pub token: Option<String>,
    /// Base URL for API calls if status is "confirmed"
    #[serde(default)]
    pub base_url: Option<String>,
    /// Error message if status is "error"
    #[serde(default)]
    pub error: Option<String>,
}

// ============================================================================
// Tool 动作的 Payload 类型
// ============================================================================

/// execute_tool 的输入参数
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct ExecuteToolInput {
    pub name: String,
    pub arguments: std::collections::HashMap<String, serde_json::Value>,
}

/// execute_tool 的输出结果
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct ExecuteToolOutput {
    pub result: String,
    pub error: Option<String>,
}

/// get_tools 的输入参数（空）
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct GetToolsInput {}
