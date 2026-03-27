//! Weixin Protocol Constants
//!
//! Constants for the WeChat iLink HTTP API protocol.

/// Base URL for Weixin iLink API
pub const BASE_URL: &str = "https://ilinkai.weixin.qq.com";

/// Base URL for Weixin CDN (media upload/download)
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

/// Channel version sent with each request
pub const CHANNEL_VERSION: &str = "1.0.3";

/// Message item types
pub const ITEM_TEXT: i32 = 1;
pub const ITEM_IMAGE: i32 = 2;
pub const ITEM_VOICE: i32 = 3;
pub const ITEM_FILE: i32 = 4;
pub const ITEM_VIDEO: i32 = 5;

/// Message type constants
pub const MESSAGE_TYPE_USER: i32 = 1;
pub const MESSAGE_TYPE_BOT: i32 = 2;

/// Message state constants
pub const MESSAGE_STATE_FINISH: i32 = 2;

/// Maximum message content length
pub const WEIXIN_MAX_MESSAGE_LEN: usize = 4000;

/// Default timeout for long-polling requests (seconds)
pub const DEFAULT_LONG_POLL_TIMEOUT_S: u64 = 35;

/// Media upload types
pub const UPLOAD_MEDIA_IMAGE: i32 = 1;
pub const UPLOAD_MEDIA_VIDEO: i32 = 2;
pub const UPLOAD_MEDIA_FILE: i32 = 3;
