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
#[serde(tag = "type", content = "data")]
#[encoding(Msgpack)]
pub enum Effect {
    #[serde(rename = "http_request")]
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
// LambdaInput / LambdaOutput - 插件入口参数和返回值
// ============================================================================

/// 插件入口参数
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct LambdaInput {
    /// 版本号，用于未来兼容
    pub version: u32,
    /// 要执行的动作
    pub action: Action,
    /// MessagePack - 插件上次返回的状态
    pub state: Vec<u8>,
    /// MessagePack - action 对应的参数
    pub payload: Vec<u8>,
    /// 之前执行的 Effect 结果列表（插件可根据此决定下一步）
    #[serde(default)]
    pub effect_results: Vec<EffectResult>,
}

/// 插件返回值
#[derive(Serialize, Deserialize, Debug, Clone, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct LambdaOutput {
    /// 需要主机执行的 Effect 列表（并行执行）
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// MessagePack - action 的结果
    pub result: Vec<u8>,
    /// MessagePack - 插件返回的新状态
    pub new_state: Vec<u8>,
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
