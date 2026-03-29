//! Host randomness functions - safe wrappers around extism host calls
//!
//! These provide cryptographically secure random bytes from the host.

use extism_pdk::*;

/// Declare external host functions for randomness (provided by mochiclaw-core)
#[host_fn]
extern "ExtismHost" {
    /// Returns a cryptographically random u64
    fn host_rand_u64() -> u64;

    /// Allocates memory in lambda, fills with random bytes, returns offset
    fn host_rand_bytes(len: u64) -> u64;
}

/// Returns a cryptographically random u32
pub fn rand_u32() -> u32 {
    match unsafe { host_rand_u64() } {
        Ok(v) => v as u32,
        Err(e) => {
            error!("host_rand_u64 failed: {:?}", e);
            0
        }
    }
}

/// Returns a cryptographically random u64
pub fn rand_u64() -> u64 {
    let offset = unsafe { host_rand_u64() }.unwrap_or(0);
    if offset == 0 {
        return 0;
    }

    if let Some(memory) = Memory::find(offset) {
        let bytes = memory.to_vec();
        let val = u64::from_le_bytes(bytes.try_into().unwrap_or([0u8; 8]));
        memory.free();
        val
    } else {
        0
    }
}

/// Fill the given buffer with cryptographically random bytes
pub fn rand_bytes(buf: &mut [u8]) {
    if buf.is_empty() {
        return;
    }
    let len = buf.len() as u64;
    let offset = match unsafe { host_rand_bytes(len) } {
        Ok(v) => v,
        Err(e) => {
            error!("host_rand_bytes failed: {:?}", e);
            return;
        }
    };

    if let Some(memory) = Memory::find(offset) {
        let bytes = memory.to_vec();
        let copy_len = buf.len().min(bytes.len());
        buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
        memory.free();
    }
}

/// Generate a random u32 in the range [0, end)
pub fn rand_u32_bounded(end: u32) -> u32 {
    if end == 0 {
        return 0;
    }
    // Simple rejection sampling for uniformity
    let mut value = rand_u32();
    let limit = u32::MAX - (u32::MAX % end);
    while value >= limit {
        value = rand_u32();
    }
    value % end
}
