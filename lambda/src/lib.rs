//! Mochiclaw Lambda
//!
//! Lambda host, manifest, and registry for managing extism WASM lambdas.

pub mod context;
pub mod discover;
pub mod error;
pub mod host;
pub mod lambda;
pub mod manifest;

pub use context::{LambdaContext, LambdaContextMap};
pub use discover::discover;
pub use error::Error;
pub use host::kv::LambdaKV;
pub use lambda::LambdaHost;
pub use manifest::{Capabilities, Features, LambdaManifest};
