//! Channel trait - implemented by chat platform plugins

use async_trait::async_trait;
use crate::error::Error;
use crate::message::OutboundMessage;

/// Channel plugin trait
///
/// Implement this trait to create a new channel plugin (e.g., Weixin, Telegram).
/// The channel is responsible for:
/// - Connecting to the chat platform
/// - Receiving inbound messages
/// - Sending outbound messages
#[async_trait]
pub trait Channel: Send + Sync {
    /// Unique name of this channel (e.g., "weixin", "telegram")
    fn name(&self) -> &str;

    /// Human-readable display name
    fn display_name(&self) -> &str {
        self.name()
    }

    /// Start the channel (begin listening)
    async fn start(&self) -> Result<(), Error>;

    /// Stop the channel (cleanup)
    async fn stop(&self) -> Result<(), Error>;

    /// Send a message through this channel
    async fn send(&self, msg: OutboundMessage) -> Result<(), Error>;
}
