//! Mochiclaw Core
//!
//! Core runtime components: MessageBus, AgentLoop.

pub mod agent_loop;
pub mod bus;
pub mod commands;
pub mod context;
pub mod error;
pub mod http_executor;
pub mod lambda_loop;
pub mod poller;
pub mod session;

pub use agent_loop::AgentLoop;
pub use bus::MessageBus;
pub use commands::{CommandRegistry, parse_command};
pub use context::ContextBuilder;
pub use error::Error;
pub use http_executor::AsyncHttpExecutor;
pub use session::{Message, Session, SessionManager};

// Re-export config from mochiclaw-config
pub use mochiclaw_config::Config;

// Re-export lambda types from mochiclaw-lambda
pub use mochiclaw_lambda::{LambdaHost, LambdaManifest, discover};
