//! Mochiclaw Core
//!
//! Core runtime components: MessageBus, AgentLoop, Config.

pub mod agent;
pub mod bus;
pub mod commands;
pub mod config;
pub mod error;
pub mod session;

pub use agent::AgentLoop;
pub use bus::MessageBus;
pub use commands::{CommandRegistry, parse_command};
pub use config::Config;
pub use error::Error;
pub use session::{Session, SessionManager, Message};

// Re-export plugin types from mochiclaw-plugin
pub use mochiclaw_plugin::{discover, PluginHost, PluginManifest};
