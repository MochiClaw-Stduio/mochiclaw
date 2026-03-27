//! Outbound message from the agent to a channel

use serde::{Deserialize, Serialize};

/// Outbound message from the agent to a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub chat_id: String,
    pub content: String,
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub media: Vec<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}
