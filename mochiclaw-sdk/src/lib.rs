//! Mochiclaw SDK
//!
//! Shared types for both host and plugins.

pub mod command;
pub mod error;
pub mod host;
pub mod message;
pub mod provider;
pub mod tool;

pub use command::{CommandInfo, CommandRequest, CommandResponse};
pub use error::Error;
pub use message::{InboundMessage, OutboundMessage};
pub use provider::{ChatRequest, ChatResponse, Message, MessageRole, ToolCall};
pub use tool::Tool;
