//! Weixin API Types
//!
//! Type definitions for WeChat iLink HTTP API requests and responses.

use mochiclaw_sdk::InboundMessage;
use serde::{Deserialize, Serialize};

// ============================================================================
// Plugin-facing Request/Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub status: String,
    pub qr_url: Option<String>,
    pub temp_token: Option<String>,
    pub token: Option<String>,
    pub base_url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginParams {
    pub config_json: String,
}

#[derive(Debug, Serialize)]
pub struct QrStatusResponse {
    pub status: String,
    pub token: Option<String>,
    pub base_url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QrStatusParams {
    pub temp_token: String,
}

#[derive(Debug, Serialize)]
pub struct PollResponse {
    pub messages: Vec<InboundMessage>,
    pub get_updates_buf: String,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PollParams {
    pub token: String,
    pub get_updates_buf: String,
}

#[derive(Debug, Serialize)]
pub struct SendResponse {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendTextParams {
    pub token: String,
    pub to_user_id: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub upload_param: String,
    pub aes_key: String,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetUploadUrlParams {
    pub token: String,
    pub to_user_id: String,
    pub file_data_base64: String,
    pub file_name: String,
    pub media_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SendMediaParams {
    pub token: String,
    pub to_user_id: String,
    pub content: String,
    pub context_token: String,
    pub media_type: String,
    pub download_param: String,
    pub aes_key: String,
    pub file_name: String,
    pub file_size: usize,
}

#[derive(Debug, Deserialize)]
pub struct GetConfigParams {
    pub token: String,
    pub ilink_user_id: String,
    pub context_token: String,
}

#[derive(Debug, Serialize)]
pub struct GetConfigResponse {
    pub success: bool,
    pub typing_ticket: String,
    pub error: Option<String>,
}

/// Parameters for set_typing (generic boolean interface)
#[derive(Debug, Deserialize)]
pub struct SetTypingParams {
    pub token: String,
    pub chat_id: String,
    pub typing: bool, // true=start typing, false=stop typing
}

// ============================================================================
// Internal API Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct WeixinApiResponse {
    #[serde(default)]
    pub ret: Option<i32>,
    #[serde(default)]
    pub errcode: Option<i32>,
    #[serde(default)]
    pub msgs: Vec<serde_json::Value>,
    #[serde(default)]
    pub get_updates_buf: String,
}

#[derive(Debug, Deserialize)]
pub struct QrStatusResponseInternal {
    pub status: String,
    pub bot_token: String,
    pub baseurl: String,
}

// ============================================================================
// Result Types for Internal Use
// ============================================================================

#[derive(Debug, Serialize)]
pub struct QrStatusResult {
    pub status: String,
    pub token: Option<String>,
    pub base_url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PollResult {
    pub messages: Vec<InboundMessage>,
    pub get_updates_buf: String,
    pub error: Option<String>,
}

// ============================================================================
// Config
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct WeixinConfig {
    pub token: String,
    pub base_url: String,
    pub get_updates_buf: String,
    #[serde(default)]
    pub route_tag: String,
}

impl WeixinConfig {
    pub fn load(path: &str) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, content).map_err(|e| e.to_string())
    }
}
