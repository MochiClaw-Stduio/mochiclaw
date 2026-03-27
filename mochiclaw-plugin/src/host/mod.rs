//! Host functions provided by the runtime

use extism::Function;

pub mod random;

pub use random::{rand_bytes_fn, rand_u64_fn};

/// Create all rand host functions
pub fn host_functions() -> Vec<Function> {
    vec![rand_u64_fn(), rand_bytes_fn()]
}
