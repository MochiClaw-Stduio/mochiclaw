//! Host functions provided by the runtime

use extism::Function;
use std::sync::Arc;

pub mod kv;
pub mod random;

pub use kv::{PluginKV, kv_functions};
pub use random::rand_functions;

/// Builder for host functions
#[derive(Default)]
pub struct HostFunctionsBuilder {
    kv: Option<(Arc<PluginKV>, String, Vec<String>)>, // (kv, plugin_name, allowed_kv_read)
}

impl HostFunctionsBuilder {
    pub fn new() -> Self {
        Self { kv: None }
    }

    pub fn with_kv(
        mut self,
        kv: Arc<PluginKV>,
        plugin_name: &str,
        allowed_kv_read: Vec<String>,
    ) -> Self {
        self.kv = Some((kv, plugin_name.to_string(), allowed_kv_read));
        self
    }

    pub fn build(self) -> Vec<Function> {
        let mut funcs = rand_functions();
        if let Some((kv, plugin_name, allowed_kv_read)) = self.kv {
            funcs.extend(kv_functions(kv, &plugin_name, allowed_kv_read));
        }
        funcs
    }
}
