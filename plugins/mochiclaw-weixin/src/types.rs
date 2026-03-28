//! Weixin API Types
//!
//! Type definitions for WeChat iLink HTTP API requests and responses.

use mochiclaw_sdk::message::InboundMessage;
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
    pub config: Vec<u8>,
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
        let content = std::fs::read(path).ok()?;
        rmp_serde::from_slice(&content).ok()
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let content = rmp_serde::to_vec(self).map_err(|e| e.to_string())?;
        std::fs::write(path, content).map_err(|e| e.to_string())
    }
}
