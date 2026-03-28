//! Host functions provided by the runtime

pub mod http;
pub mod kv;
pub mod random;

pub use http::{HttpClient, HttpError, HttpResponse};
pub use kv::{kv_get, kv_get_raw, kv_remove, kv_set, kv_set_raw};
pub use random::{rand_bytes, rand_u32, rand_u32_bounded, rand_u64};
