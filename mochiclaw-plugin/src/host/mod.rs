//! Host functions provided by the runtime

use extism::Function;
use std::sync::Arc;

pub mod fs;
pub mod http;
pub mod kv;
pub mod random;

pub use fs::{fs_functions, FsContext};
pub use http::{http_functions, HttpContext};
pub use kv::{PluginKV, kv_functions};
pub use random::rand_functions;

/// Builder for host functions
#[derive(Default)]
pub struct HostFunctionsBuilder {
    kv: Option<(Arc<PluginKV>, String, Vec<String>)>, // (kv, plugin_name, allowed_kv_read)
    http: Option<HttpContext>,
    fs: Option<FsContext>,
}

impl HostFunctionsBuilder {
    pub fn new() -> Self {
        Self { kv: None, http: None, fs: None }
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

    pub fn with_http(mut self, ctx: HttpContext) -> Self {
        self.http = Some(ctx);
        self
    }

    pub fn with_fs(mut self, ctx: FsContext) -> Self {
        self.fs = Some(ctx);
        self
    }

    pub fn build(self) -> Vec<Function> {
        let mut funcs = rand_functions();
        if let Some((kv, plugin_name, allowed_kv_read)) = self.kv {
            funcs.extend(kv_functions(kv, &plugin_name, allowed_kv_read));
        }
        if let Some(ctx) = self.http {
            funcs.extend(http_functions(ctx));
        }
        if let Some(ctx) = self.fs {
            funcs.extend(fs_functions(ctx));
        }
        funcs
    }
}
