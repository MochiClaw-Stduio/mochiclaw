//! Generic channel plugin types
//!
//! Common request/response types for channel plugins (e.g., WeChat, Telegram, Discord).
//! These types are channel-agnostic and can be reused across different channel implementations.

use serde::{Deserialize, Serialize};
use extism_convert::{FromBytes, ToBytes, Msgpack};

use crate::message::InboundMessage;

// ============================================================================
// Login / QR Code Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize, ToBytes)]
#[encoding(Msgpack)]
pub struct QrStatusResponse {
    pub status: String,
    pub token: Option<String>,
    pub base_url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, FromBytes)]
#[encoding(Msgpack)]
pub struct QrStatusParams {
    pub temp_token: String,
}

// ============================================================================
// Poll Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize, ToBytes)]
#[encoding(Msgpack)]
pub struct PollResponse {
    pub messages: Vec<InboundMessage>,
    pub get_updates_buf: String,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, FromBytes)]
#[encoding(Msgpack)]
pub struct PollParams {
    pub token: String,
    #[serde(default)]
    pub get_updates_buf: String,
}

// ============================================================================
// Send Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize, ToBytes)]
#[encoding(Msgpack)]
pub struct SendResponse {
    pub success: bool,
    pub error: Option<String>,
}

// ============================================================================
// Typing Indicator Types
// ============================================================================

#[derive(Debug, Deserialize, FromBytes)]
#[encoding(Msgpack)]
pub struct SetTypingParams {
    pub token: String,
    pub chat_id: String,
    pub typing: bool,
}
