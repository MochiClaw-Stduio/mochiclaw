//! Mochiclaw Plugin
//!
//! Plugin host, manifest, and registry for managing extism WASM plugins.

pub mod discover;
pub mod error;
pub mod host;
pub mod manifest;
pub mod plugin;

pub use discover::discover;
pub use error::Error;
pub use manifest::{Capabilities, Features, PluginManifest};
pub use plugin::PluginHost;
