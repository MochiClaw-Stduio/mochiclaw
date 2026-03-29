//! Inbound message from a channel to the agent

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Inbound message from a channel to the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    #[serde(default)]
    pub channel: String,
    pub sender_id: String,
    pub chat_id: String,
    pub content: String,
    #[serde(default)]
    pub media: Vec<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub session_key_override: Option<String>,
}

impl InboundMessage {
    pub fn session_key(&self) -> Cow<'_, str> {
        if let Some(ref key) = self.session_key_override {
            Cow::Borrowed(key.as_str())
        } else {
            Cow::Owned(format!("{}:{}", self.channel, self.chat_id))
        }
    }
}
