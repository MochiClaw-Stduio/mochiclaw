//! Mochiclaw Lambda
//!
//! Lambda host, manifest, and registry for managing extism WASM plugins.

pub mod context;
pub mod discover;
pub mod error;
pub mod host;
pub mod manifest;
pub mod plugin;

pub use context::{PluginContext, PluginContextMap};
pub use discover::discover;
pub use error::Error;
pub use host::kv::PluginKV;
pub use manifest::{Capabilities, Features, PluginManifest};
pub use plugin::PluginHost;
