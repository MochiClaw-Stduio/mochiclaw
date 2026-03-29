//! Host KV functions - safe wrappers for lambda use
//!
//! Provides typed access to the shared KV store from lambdas.

use extism_pdk::*;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// KV input structures (MessagePack encoded)
#[derive(Serialize, Deserialize)]
struct KVGetInput {
    /// Target lambda to read from (optional, defaults to own lambda if None/empty)
    lambda: Option<String>,
    key: String,
}

#[derive(Serialize, Deserialize)]
struct KVSetInput {
    key: String,
    value: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct KVRemoveInput {
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

    /// List lambdas this lambda can read from
    ///
    /// Output: MessagePack encoded Vec<String>
    fn host_kv_list_readable() -> Vec<u8>;

    /// List lambdas this lambda can write to
    ///
    /// Output: MessagePack encoded Vec<String>
    fn host_kv_list_writable() -> Vec<u8>;
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
/// This reads from the calling lambda's own KV store.
///
/// # Arguments
/// * `key` - key to get
///
/// # Returns
/// * `Some(T)` on success
/// * `None` if key not found or deserialization failed
pub fn kv_get<T: for<'de> Deserialize<'de>>(key: &str) -> Option<T> {
    let input = KVGetInput {
        lambda: None,
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

/// Get a value from another lambda's KV store and deserialize it
///
/// Requires the calling lambda to have permission (declared in allowed_kv_read).
///
/// # Arguments
/// * `lambda` - lambda name to read from
/// * `key` - key to get
///
/// # Returns
/// * `Some(T)` on success
/// * `None` if key not found, not allowed, or deserialization failed
pub fn kv_get_from<T: for<'de> Deserialize<'de>>(lambda: &str, key: &str) -> Option<T> {
    let input = KVGetInput {
        lambda: Some(lambda.to_string()),
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
/// This writes to the calling lambda's own KV store.
///
/// # Arguments
/// * `key` - key to set
/// * `value` - value to store (will be MessagePack serialized)
///
/// # Returns
/// * `true` on success
/// * `false` on failure
pub fn kv_set<T: Serialize>(key: &str, value: &T) -> bool {
    let value_bytes = match to_msgpack(value) {
        Some(b) => b,
        None => return false,
    };

    let input = KVSetInput {
        key: key.to_string(),
        value: value_bytes,
    };

    let input_bytes = match to_msgpack(&input) {
        Some(b) => b,
        None => return false,
    };

    matches!(unsafe { host_kv_set(input_bytes) }, Ok(0))
}
/// Set a raw bytes value in KV store (no serialization)
///
/// # Arguments
/// * `key` - key to set
/// * `value` - raw bytes to store
///
/// # Returns
/// * `true` on success
/// * `false` on failure
pub fn kv_set_raw(key: &str, value: Vec<u8>) -> bool {
    let input = KVSetInput {
        key: key.to_string(),
        value,
    };

    let input_bytes = match to_msgpack(&input) {
        Some(b) => b,
        None => return false,
    };

    matches!(unsafe { host_kv_set(input_bytes) }, Ok(0))
}
/// Get a raw bytes value from KV store (no deserialization)
///
/// This reads from the calling lambda's own KV store.
///
/// # Arguments
/// * `key` - key to get
///
/// # Returns
/// * `Some(Vec<u8>)` on success
/// * `None` if key not found
pub fn kv_get_raw(key: &str) -> Option<Vec<u8>> {
    let input = KVGetInput {
        lambda: None,
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
/// This removes from the calling lambda's own KV store.
///
/// # Arguments
/// * `key` - key to remove
///
/// # Returns
/// * `true` on success (key was present)
/// * `false` on failure
pub fn kv_remove(key: &str) -> bool {
    let input = KVRemoveInput {
        key: key.to_string(),
    };

    let input_bytes = match to_msgpack(&input) {
        Some(b) => b,
        None => return false,
    };

    matches!(unsafe { host_kv_remove(input_bytes) }, Ok(0))
}

/// List lambdas this lambda can read from (including self).
///
/// # Returns
/// * `Some(Vec<String>)` on success
/// * `None` on failure
pub fn kv_list_readable() -> Option<Vec<String>> {
    let output_bytes = match unsafe { host_kv_list_readable() } {
        Ok(b) => b,
        Err(_) => return None,
    };

    if output_bytes.is_empty() {
        return None;
    }

    from_msgpack(&output_bytes)
}

/// List lambdas this lambda can write to (currently just self).
///
/// # Returns
/// * `Some(Vec<String>)` on success
/// * `None` on failure
pub fn kv_list_writable() -> Option<Vec<String>> {
    let output_bytes = match unsafe { host_kv_list_writable() } {
        Ok(b) => b,
        Err(_) => return None,
    };

    if output_bytes.is_empty() {
        return None;
    }

    from_msgpack(&output_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmp_serde::Serializer;

    /// Test msgpack serialization roundtrip for various types
    #[test]
    fn test_msgpack_roundtrip_string() {
        let value = "hello world";
        let serialized = to_msgpack(&value).unwrap();
        let deserialized: String = from_msgpack(&serialized).unwrap();
        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_msgpack_roundtrip_u64() {
        let value: u64 = 12345678901234567890;
        let serialized = to_msgpack(&value).unwrap();
        let deserialized: u64 = from_msgpack(&serialized).unwrap();
        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_msgpack_roundtrip_vec_u8() {
        let value: Vec<u8> = vec![1, 2, 3, 4, 5, 255, 0];
        let serialized = to_msgpack(&value).unwrap();
        let deserialized: Vec<u8> = from_msgpack(&serialized).unwrap();
        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_msgpack_roundtrip_option_vec_u8_some() {
        let value: Option<Vec<u8>> = Some(vec![1, 2, 3]);
        let mut buf = Vec::new();
        value.serialize(&mut Serializer::new(&mut buf)).unwrap();
        let deserialized: Option<Vec<u8>> = from_msgpack(&buf).unwrap();
        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_msgpack_roundtrip_option_vec_u8_none() {
        let value: Option<Vec<u8>> = None;
        let mut buf = Vec::new();
        value.serialize(&mut Serializer::new(&mut buf)).unwrap();
        let deserialized: Option<Vec<u8>> = from_msgpack(&buf).unwrap();
        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_kv_get_input_serialization() {
        // Test with lambda = None (read from self)
        let input = KVGetInput {
            lambda: None,
            key: "test_key".to_string(),
        };
        let serialized = to_msgpack(&input).unwrap();
        let deserialized: KVGetInput = from_msgpack(&serialized).unwrap();
        assert_eq!(input.lambda, deserialized.lambda);
        assert_eq!(input.key, deserialized.key);

        // Test with lambda = Some (read from other)
        let input = KVGetInput {
            lambda: Some("other_lambda".to_string()),
            key: "test_key".to_string(),
        };
        let serialized = to_msgpack(&input).unwrap();
        let deserialized: KVGetInput = from_msgpack(&serialized).unwrap();
        assert_eq!(input.lambda, deserialized.lambda);
        assert_eq!(input.key, deserialized.key);
    }

    #[test]
    fn test_kv_set_input_serialization() {
        let input = KVSetInput {
            key: "my_key".to_string(),
            value: vec![1, 2, 3, 4],
        };
        let serialized = to_msgpack(&input).unwrap();
        let deserialized: KVSetInput = from_msgpack(&serialized).unwrap();
        assert_eq!(input.key, deserialized.key);
        assert_eq!(input.value, deserialized.value);
    }

    #[test]
    fn test_kv_remove_input_serialization() {
        let input = KVRemoveInput {
            key: "to_delete".to_string(),
        };
        let serialized = to_msgpack(&input).unwrap();
        let deserialized: KVRemoveInput = from_msgpack(&serialized).unwrap();
        assert_eq!(input.key, deserialized.key);
    }

    #[test]
    fn test_nested_serialization() {
        // Simulate storing a struct in KV
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct UserData {
            name: String,
            age: u32,
        }

        let user = UserData {
            name: "Alice".to_string(),
            age: 30,
        };

        // Wrap in Option<Vec<u8>>
        let wrapped: Option<Vec<u8>> = to_msgpack(&user);
        let wrapped_bytes = wrapped.unwrap();

        // Deserialize back
        let unwrapped: Option<UserData> = from_msgpack(&wrapped_bytes);
        let user_back = unwrapped.unwrap();

        assert_eq!(user, user_back);
    }

    #[test]
    fn test_empty_key_handling() {
        let input = KVGetInput {
            lambda: None,
            key: "".to_string(),
        };
        let serialized = to_msgpack(&input).unwrap();
        let deserialized: KVGetInput = from_msgpack(&serialized).unwrap();
        assert_eq!(input.key, deserialized.key);
    }

    #[test]
    fn test_empty_lambda_means_self() {
        let input = KVGetInput {
            lambda: Some("".to_string()),
            key: "test".to_string(),
        };
        let serialized = to_msgpack(&input).unwrap();
        let deserialized: KVGetInput = from_msgpack(&serialized).unwrap();
        // Empty string lambda should be preserved (host decides what it means)
        assert_eq!(input.lambda, deserialized.lambda);
    }
}
