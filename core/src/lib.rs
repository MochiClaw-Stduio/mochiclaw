//! Mochiclaw Core
//!
//! Core runtime components: MessageBus, AgentLoop.

pub mod agent;
pub mod bus;
pub mod commands;
pub mod context;
pub mod error;
pub mod session;

pub use agent::AgentLoop;
pub use bus::MessageBus;
pub use commands::{CommandRegistry, parse_command};
pub use context::ContextBuilder;
pub use error::Error;
pub use session::{Message, Session, SessionManager};

// Re-export config from mochiclaw-config
pub use mochiclaw_config::Config;

// Re-export plugin types from mochiclaw-plugin
pub use mochiclaw_plugin::{PluginHost, PluginManifest, discover};
