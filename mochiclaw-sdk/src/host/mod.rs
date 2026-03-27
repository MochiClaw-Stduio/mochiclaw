//! Host functions provided by the runtime

pub mod random;

pub use random::{rand_bytes, rand_u32, rand_u32_bounded, rand_u64};
