//! Host KV functions - safe wrappers for plugin use
//!
//! Provides typed access to the shared KV store from plugins.

use extism_pdk::*;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// KV input structures (MessagePack encoded)
#[derive(Serialize, Deserialize)]
struct KVGetInput {
    plugin: String,
    user: String,
    key: String,
}

#[derive(Serialize, Deserialize)]
struct KVSetInput {
    plugin: String,
    user: String,
    key: String,
    value: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct KVRemoveInput {
    plugin: String,
    user: String,
    key: String,
}

/// Declare external host functions for KV store (provided by mochiclaw-core)
#[host_fn]
extern "ExtismHost" {
    /// Get a value from KV store
    ///
    /// Input: MessagePack encoded KVGetInput
    /// Output: MessagePack encoded Option<Vec<u8>>
    fn host_kv_get(input: Vec<u8>) -> Vec<u8>;

    /// Set a value in KV store
    ///
    /// Input: MessagePack encoded KVSetInput
    /// Output: i64 (0 = success, -1 = failed)
    fn host_kv_set(input: Vec<u8>) -> i64;

    /// Remove a value from KV store
    ///
    /// Input: MessagePack encoded KVRemoveInput
    /// Output: i64 (0 = success, -1 = not found)
    fn host_kv_remove(input: Vec<u8>) -> i64;
}

/// Serialize a value to a byte vector using MessagePack
fn to_msgpack<T: Serialize>(value: &T) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    value.serialize(&mut Serializer::new(&mut buf)).ok()?;
    Some(buf)
}

/// Deserialize a value from a byte slice using MessagePack
fn from_msgpack<'a, T: Deserialize<'a>>(buf: &'a [u8]) -> Option<T> {
    T::deserialize(&mut Deserializer::new(Cursor::new(buf))).ok()
}

/// Get a value from KV store and deserialize it
///
/// # Arguments
/// * `plugin` - plugin name
/// * `user` - user identifier
/// * `key` - key to get
///
/// # Returns
/// * `Some(T)` on success
/// * `None` if key not found or deserialization failed
pub fn kv_get<T: for<'de> Deserialize<'de>>(plugin: &str, user: &str, key: &str) -> Option<T> {
    let input = KVGetInput {
        plugin: plugin.to_string(),
        user: user.to_string(),
        key: key.to_string(),
    };

    let input_bytes = to_msgpack(&input)?;

    let output_bytes = match unsafe { host_kv_get(input_bytes) } {
        Ok(b) => b,
        Err(_) => return None,
    };

    if output_bytes.is_empty() {
        return None;
    }

    // Deserialize Option<Vec<u8>>
    let value: Option<Vec<u8>> = from_msgpack(&output_bytes)?;

    match value {
        Some(bytes) => from_msgpack(&bytes),
        None => None,
    }
}

/// Set a value in KV store (serializes to MessagePack first)
///
/// # Arguments
/// * `plugin` - plugin name
/// * `user` - user identifier
/// * `key` - key to set
/// * `value` - value to store (will be MessagePack serialized)
///
/// # Returns
/// * `true` on success
/// * `false` on failure
pub fn kv_set<T: Serialize>(plugin: &str, user: &str, key: &str, value: &T) -> bool {
    let value_bytes = match to_msgpack(value) {
        Some(b) => b,
        None => return false,
    };

    let input = KVSetInput {
        plugin: plugin.to_string(),
        user: user.to_string(),
        key: key.to_string(),
        value: value_bytes,
    };

    let input_bytes = match to_msgpack(&input) {
        Some(b) => b,
        None => return false,
    };

    match unsafe { host_kv_set(input_bytes) } {
        Ok(0) => true,
        _ => false,
    }
}

/// Set a raw bytes value in KV store (no serialization)
///
/// # Arguments
/// * `plugin` - plugin name
/// * `user` - user identifier
/// * `key` - key to set
/// * `value` - raw bytes to store
///
/// # Returns
/// * `true` on success
/// * `false` on failure
pub fn kv_set_raw(plugin: &str, user: &str, key: &str, value: Vec<u8>) -> bool {
    let input = KVSetInput {
        plugin: plugin.to_string(),
        user: user.to_string(),
        key: key.to_string(),
        value,
    };

    let input_bytes = match to_msgpack(&input) {
        Some(b) => b,
        None => return false,
    };

    match unsafe { host_kv_set(input_bytes) } {
        Ok(0) => true,
        _ => false,
    }
}

/// Get a raw bytes value from KV store (no deserialization)
///
/// # Arguments
/// * `plugin` - plugin name
/// * `user` - user identifier
/// * `key` - key to get
///
/// # Returns
/// * `Some(Vec<u8>)` on success
/// * `None` if key not found
pub fn kv_get_raw(plugin: &str, user: &str, key: &str) -> Option<Vec<u8>> {
    let input = KVGetInput {
        plugin: plugin.to_string(),
        user: user.to_string(),
        key: key.to_string(),
    };

    let input_bytes = to_msgpack(&input)?;

    let output_bytes = match unsafe { host_kv_get(input_bytes) } {
        Ok(b) => b,
        Err(_) => return None,
    };

    if output_bytes.is_empty() {
        return None;
    }

    // Deserialize Option<Vec<u8>>
    from_msgpack(&output_bytes)
}

/// Remove a value from KV store
///
/// # Arguments
/// * `plugin` - plugin name
/// * `user` - user identifier
/// * `key` - key to remove
///
/// # Returns
/// * `true` on success (key was present)
/// * `false` on failure
pub fn kv_remove(plugin: &str, user: &str, key: &str) -> bool {
    let input = KVRemoveInput {
        plugin: plugin.to_string(),
        user: user.to_string(),
        key: key.to_string(),
    };

    let input_bytes = match to_msgpack(&input) {
        Some(b) => b,
        None => return false,
    };

    match unsafe { host_kv_remove(input_bytes) } {
        Ok(0) => true,
        _ => false,
    }
}
