//! Generic channel lambda types
//!
//! Common request/response types for channel lambdas (e.g., WeChat, Telegram, Discord).
//! These types are channel-agnostic and can be reused across different channel implementations.

use extism_convert::{FromBytes, Msgpack, ToBytes};
use serde::{Deserialize, Serialize};

use crate::message::InboundMessage;

// ============================================================================
// Login Types
// ============================================================================

/// Parameters for the login function - generic for all channel lambdas
#[derive(Debug, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct LoginParams {
    /// Lambda-specific config bytes (e.g., previously saved token, credentials)
    pub config: Vec<u8>,
}

/// Response from the login function - generic for all channel lambdas
#[derive(Debug, Clone, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct LoginResponse {
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

// ============================================================================
// QR Code Status Types
// ============================================================================

/// Response from checking QR code scan status - generic for all channel lambdas
#[derive(Debug, Clone, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct QrStatusResponse {
    /// Status: "confirmed", "scaned", "expired", "error", or continue polling
    pub status: String,
    /// Auth token if status is "confirmed"
    pub token: Option<String>,
    /// Base URL for API calls if status is "confirmed"
    pub base_url: Option<String>,
    /// Error message if status is "error"
    pub error: Option<String>,
}

/// Parameters for checking QR code scan status
#[derive(Debug, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct QrStatusParams {
    pub temp_token: String,
}

// ============================================================================
// Poll Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct PollResponse {
    pub messages: Vec<InboundMessage>,
    pub get_updates_buf: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct PollParams {
    pub token: String,
    #[serde(default)]
    pub get_updates_buf: String,
}

// ============================================================================
// Send Types
// ============================================================================

/// Parameters for sending a text message - generic for all channel lambdas
#[derive(Debug, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct SendTextParams {
    pub token: String,
    pub to_user_id: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct SendResponse {
    pub success: bool,
    pub error: Option<String>,
}

// ============================================================================
// Typing Indicator Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct SetTypingParams {
    pub token: String,
    pub chat_id: String,
    pub typing: bool,
}
