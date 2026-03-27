//! Weixin Channel Plugin
//!
//! This plugin handles:
//! - QR code login for WeChat
//! - HTTP long-polling to receive messages
//! - Sending text and media messages
//! - AES-128-ECB encryption/decryption for media
//!
//! Uses extism's built-in http::request for HTTP calls.

mod api;
mod constants;
mod crypto;
mod messages;
mod session;
mod types;

// Re-export for backward compatibility
pub use constants::*;
pub use types::WeixinConfig;

// Re-export plugin functions
pub use api::{check_login, get_config, get_name, get_upload_url, login, parse_update, poll, send_media, send_text, send_typing};
