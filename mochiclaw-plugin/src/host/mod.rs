//! Host functions provided by the runtime

use extism::Function;
use std::sync::Arc;

pub mod kv;
pub mod random;

pub use kv::{kv_functions, PluginKV};
pub use random::{rand_bytes_fn, rand_u64_fn};

/// Builder for host functions
#[derive(Default)]
pub struct HostFunctionsBuilder {
    kv: Option<Arc<PluginKV>>,
}

impl HostFunctionsBuilder {
    pub fn new() -> Self {
        Self { kv: None }
    }

    pub fn with_kv(mut self, kv: Arc<PluginKV>) -> Self {
        self.kv = Some(kv);
        self
    }

    pub fn build(self) -> Vec<Function> {
        let mut funcs = vec![rand_u64_fn(), rand_bytes_fn()];
        if let Some(kv) = self.kv {
            funcs.extend(kv_functions(kv));
        }
        funcs
    }
}
