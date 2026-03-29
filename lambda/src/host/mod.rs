//! Host functions provided by the runtime

use extism::Function;
use std::sync::Arc;

pub mod fs;
pub mod kv;
pub mod random;

pub use fs::{FsContext, fs_functions};
pub use kv::{LambdaKV, kv_functions};
pub use random::rand_functions;

/// Builder for host functions
#[derive(Default)]
pub struct HostFunctionsBuilder {
    kv: Option<(Arc<LambdaKV>, String, Vec<String>)>, // (kv, lambda_name, allowed_kv_read)
    fs: Option<FsContext>,
}

impl HostFunctionsBuilder {
    pub fn new() -> Self {
        Self { kv: None, fs: None }
    }

    pub fn with_kv(
        mut self,
        kv: Arc<LambdaKV>,
        lambda_name: &str,
        allowed_kv_read: Vec<String>,
    ) -> Self {
        self.kv = Some((kv, lambda_name.to_string(), allowed_kv_read));
        self
    }

    pub fn with_fs(mut self, ctx: FsContext) -> Self {
        self.fs = Some(ctx);
        self
    }

    pub fn build(self) -> Vec<Function> {
        let mut funcs = rand_functions();
        if let Some((kv, lambda_name, allowed_kv_read)) = self.kv {
            funcs.extend(kv_functions(kv, &lambda_name, allowed_kv_read));
        }
        if let Some(ctx) = self.fs {
            funcs.extend(fs_functions(ctx));
        }
        funcs
    }
}
