//! Mochiclaw SDK
//!
//! Shared types for both host and plugins.

pub mod channel;
pub mod command;
pub mod error;
pub mod host;
pub mod message;
pub mod provider;
pub mod tool;

// Re-export extism-pdk as extism_pdk so #[plugin_fn] macro works
// (the macro generates code that references extism_pdk)
pub use extism_pdk as extism_pdk;
pub use extism_pdk::{plugin_fn, FnResult};
pub use extism_pdk::config;
pub use extism_pdk::Memory;
pub use extism_pdk::{debug, error, info, warn, trace};

// Re-export convert traits
pub use extism_convert::{FromBytes, ToBytes, Msgpack};
