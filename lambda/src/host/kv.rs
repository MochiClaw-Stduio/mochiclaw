//! Host KV functions for lambdas
//!
//! Uses MessagePack encoding for input/output structures.

use extism::{CurrentPlugin, Function, UserData, Val, ValType};
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

type KvStore = HashMap<String, Vec<u8>>;
type LambdaKvStore = HashMap<String, KvStore>;

/// Shared KV store for all lambda instances
/// Key structure: lambda_name -> key -> msgpack encoded value
pub struct LambdaKV {
    store: Arc<std::sync::Mutex<LambdaKvStore>>,
}

impl Clone for LambdaKV {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
        }
    }
}

impl LambdaKV {
    /// Create a new LambdaKV store
    pub fn new() -> Self {
        Self {
            store: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Set a value (auto-serializes to msgpack)
    pub fn set<T: Serialize>(&self, lambda: &str, key: &str, value: &T) {
        let mut buf = Vec::new();
        if value.serialize(&mut Serializer::new(&mut buf)).is_err() {
            tracing::error!("kv serialize error");
            return;
        }

        let mut store = self.store.lock().unwrap();
        store
            .entry(lambda.to_string())
            .or_default()
            .insert(key.to_string(), buf);
    }

    /// Get a value (auto-deserializes from msgpack)
    pub fn get<T: for<'de> Deserialize<'de>>(&self, lambda: &str, key: &str) -> Option<T> {
        let store = self.store.lock().unwrap();
        let bytes = store.get(lambda)?.get(key)?;

        T::deserialize(&mut Deserializer::new(Cursor::new(bytes))).ok()
    }

    /// Set raw bytes (no serialization)
    pub fn set_raw(&self, lambda: &str, key: &str, value: Vec<u8>) {
        let mut store = self.store.lock().unwrap();
        store
            .entry(lambda.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }

    /// Get raw bytes (no deserialization)
    pub fn get_raw(&self, lambda: &str, key: &str) -> Option<Vec<u8>> {
        let store = self.store.lock().unwrap();
        store.get(lambda)?.get(key).cloned()
    }

    /// Remove a value
    pub fn remove(&self, lambda: &str, key: &str) {
        let mut store = self.store.lock().unwrap();
        if let Some(m) = store.get_mut(lambda) {
            m.remove(key);
        }
    }
}

impl Default for LambdaKV {
    fn default() -> Self {
        Self::new()
    }
}

/// KV input structures for host functions (MessagePack encoded)
#[derive(Serialize, Deserialize)]
pub struct KVGetInput {
    /// Target lambda to read from (optional, defaults to own lambda if empty)
    pub lambda: Option<String>,
    /// Key to read
    pub key: String,
}

#[derive(Serialize, Deserialize)]
pub struct KVSetInput {
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct KVRemoveInput {
    pub key: String,
}

/// KV access context bound to a lambda
/// lambda_name: the lambda's own name (for write operations and self-read)
/// allowed_kv_read: list of other lambda names this lambda can read from
pub struct LambdaKVContext {
    kv: Arc<LambdaKV>,
    lambda_name: String,
    allowed_kv_read: Vec<String>,
}

impl LambdaKVContext {
    pub fn new(kv: Arc<LambdaKV>, lambda_name: &str, allowed_kv_read: Vec<String>) -> Self {
        Self {
            kv,
            lambda_name: lambda_name.to_string(),
            allowed_kv_read,
        }
    }

    /// Check if reading from target_lambda is allowed
    fn can_read(&self, target_lambda: &str) -> bool {
        target_lambda == self.lambda_name
            || self.allowed_kv_read.contains(&target_lambda.to_string())
    }
}

impl Clone for LambdaKVContext {
    fn clone(&self) -> Self {
        Self {
            kv: self.kv.clone(),
            lambda_name: self.lambda_name.clone(),
            allowed_kv_read: self.allowed_kv_read.clone(),
        }
    }
}

// Manual Send + Sync impls needed for UserData
unsafe impl Send for LambdaKVContext {}
unsafe impl Sync for LambdaKVContext {}

/// Create all KV host functions bound to a specific lambda
pub fn kv_functions(
    kv: Arc<LambdaKV>,
    lambda_name: &str,
    allowed_kv_read: Vec<String>,
) -> Vec<Function> {
    let ctx = LambdaKVContext::new(kv, lambda_name, allowed_kv_read);
    vec![
        kv_get_fn(ctx.clone()),
        kv_set_fn(ctx.clone()),
        kv_remove_fn(ctx.clone()),
        kv_list_readable_fn(ctx.clone()),
        kv_list_writable_fn(ctx),
    ]
}

/// host_kv_get: get a value from KV store
///
/// Input: MessagePack encoded KVGetInput (lambda + key)
/// Output: MessagePack encoded Option<Vec<u8>> (None if not found or not allowed)
/// Permission: own lambda always allowed, others require declared permission
pub fn kv_get_fn(ctx: LambdaKVContext) -> Function {
    Function::new(
        "host_kv_get",
        [ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        |lambda: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<LambdaKVContext>| {
            // Get context with permission info
            let ctx_arc = match user_data.get() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("kv get failed: {}", e);
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };
            let ctx_guard = match ctx_arc.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!("kv lock failed: {}", e);
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            // Read input from lambda memory
            let input_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            if input_offset == 0 {
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            let handle = match lambda.memory_handle(input_offset) {
                Some(h) => h,
                None => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            let bytes = match lambda.memory_bytes(handle) {
                Ok(b) => b.to_vec(),
                Err(_) => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            // Decode input
            let input: KVGetInput =
                match KVGetInput::deserialize(&mut Deserializer::new(Cursor::new(&bytes))) {
                    Ok(inp) => inp,
                    Err(_) => {
                        outputs[0] = Val::I64(0);
                        return Ok(());
                    }
                };

            // Default to own lambda if not specified
            let target_lambda = input
                .lambda
                .as_ref()
                .filter(|p| !p.is_empty())
                .map(|p| p.as_str())
                .unwrap_or(&ctx_guard.lambda_name);

            // Permission check: can only read from own lambda or allowed lambdas
            if !ctx_guard.can_read(target_lambda) {
                tracing::warn!(
                    "kv get denied: lambda '{}' tried to read from '{}' without permission",
                    ctx_guard.lambda_name,
                    target_lambda
                );
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            // Get raw value using LambdaKV methods (handles locking internally)
            let value = ctx_guard.kv.get_raw(target_lambda, &input.key);

            // Encode output as MessagePack Option<Vec<u8>>
            let output: Option<Vec<u8>> = value;
            let mut output_buf = Vec::new();
            if output
                .serialize(&mut Serializer::new(&mut output_buf))
                .is_err()
            {
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            if lambda.memory_set_val(&mut outputs[0], &output_buf).is_err() {
                outputs[0] = Val::I64(0);
            }
            Ok(())
        },
    )
}

/// host_kv_set: set a value in KV store
///
/// Input: MessagePack encoded KVSetInput
/// Output: i64 (0 = success, -1 = failed)
/// Permission: always writes to own lambda KV only
pub fn kv_set_fn(ctx: LambdaKVContext) -> Function {
    Function::new(
        "host_kv_set",
        [ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        |lambda: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<LambdaKVContext>| {
            // Get context
            let ctx_arc = match user_data.get() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("kv set failed: {}", e);
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };
            let ctx_guard = match ctx_arc.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!("kv lock failed: {}", e);
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            // Read input from lambda memory
            let input_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            if input_offset == 0 {
                outputs[0] = Val::I64(-1);
                return Ok(());
            }

            let handle = match lambda.memory_handle(input_offset) {
                Some(h) => h,
                None => {
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            let bytes = match lambda.memory_bytes(handle) {
                Ok(b) => b.to_vec(),
                Err(_) => {
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            // Decode input
            let input: KVSetInput =
                match KVSetInput::deserialize(&mut Deserializer::new(Cursor::new(&bytes))) {
                    Ok(inp) => inp,
                    Err(_) => {
                        outputs[0] = Val::I64(-1);
                        return Ok(());
                    }
                };

            // Always writes to own KV
            ctx_guard
                .kv
                .set_raw(&ctx_guard.lambda_name, &input.key, input.value);

            outputs[0] = Val::I64(0);
            Ok(())
        },
    )
}

/// host_kv_remove: remove a value from KV store
///
/// Input: MessagePack encoded KVRemoveInput
/// Output: i64 (0 = success, -1 = not found)
/// Permission: always removes from own lambda KV only
pub fn kv_remove_fn(ctx: LambdaKVContext) -> Function {
    Function::new(
        "host_kv_remove",
        [ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        |lambda: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<LambdaKVContext>| {
            // Get context
            let ctx_arc = match user_data.get() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("kv remove failed: {}", e);
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };
            let ctx_guard = match ctx_arc.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!("kv lock failed: {}", e);
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            // Read input from lambda memory
            let input_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            if input_offset == 0 {
                outputs[0] = Val::I64(-1);
                return Ok(());
            }

            let handle = match lambda.memory_handle(input_offset) {
                Some(h) => h,
                None => {
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            let bytes = match lambda.memory_bytes(handle) {
                Ok(b) => b.to_vec(),
                Err(_) => {
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            // Decode input
            let input: KVRemoveInput =
                match KVRemoveInput::deserialize(&mut Deserializer::new(Cursor::new(&bytes))) {
                    Ok(inp) => inp,
                    Err(_) => {
                        outputs[0] = Val::I64(-1);
                        return Ok(());
                    }
                };

            // Always removes from own KV
            ctx_guard.kv.remove(&ctx_guard.lambda_name, &input.key);

            outputs[0] = Val::I64(0);
            Ok(())
        },
    )
}

/// host_kv_list_readable: list lambdas this lambda can read from
///
/// Input: None
/// Output: MessagePack encoded Vec<String> (includes self)
pub fn kv_list_readable_fn(ctx: LambdaKVContext) -> Function {
    Function::new(
        "host_kv_list_readable",
        [],
        [ValType::I64],
        UserData::new(ctx),
        |_lambda: &mut CurrentPlugin,
         _inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<LambdaKVContext>| {
            let ctx_arc = match user_data.get() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("kv list readable failed: {}", e);
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };
            let ctx_guard = match ctx_arc.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!("kv lock failed: {}", e);
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            // Readable = self + allowed_kv_read
            let mut readable = vec![ctx_guard.lambda_name.clone()];
            readable.extend(ctx_guard.allowed_kv_read.clone());

            let mut output_buf = Vec::new();
            if readable
                .serialize(&mut Serializer::new(&mut output_buf))
                .is_err()
            {
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            if _lambda
                .memory_set_val(&mut outputs[0], &output_buf)
                .is_err()
            {
                outputs[0] = Val::I64(0);
            }
            Ok(())
        },
    )
}

/// host_kv_list_writable: list lambdas this lambda can write to
///
/// Input: None
/// Output: MessagePack encoded Vec<String> (currently just self)
pub fn kv_list_writable_fn(ctx: LambdaKVContext) -> Function {
    Function::new(
        "host_kv_list_writable",
        [],
        [ValType::I64],
        UserData::new(ctx),
        |_lambda: &mut CurrentPlugin,
         _inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<LambdaKVContext>| {
            let ctx_arc = match user_data.get() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("kv list writable failed: {}", e);
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };
            let ctx_guard = match ctx_arc.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!("kv lock failed: {}", e);
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            // Writable = only self (write to other lambdas not allowed)
            let writable = vec![ctx_guard.lambda_name.clone()];

            let mut output_buf = Vec::new();
            if writable
                .serialize(&mut Serializer::new(&mut output_buf))
                .is_err()
            {
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            if _lambda
                .memory_set_val(&mut outputs[0], &output_buf)
                .is_err()
            {
                outputs[0] = Val::I64(0);
            }
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_kv() -> LambdaKV {
        LambdaKV::new()
    }

    // =============================================================================
    // LambdaKV basic tests
    // =============================================================================

    #[test]
    fn test_lambda_kv_new_is_empty() {
        let kv = create_test_kv();
        assert!(kv.get_raw("lambda_a", "key").is_none());
    }

    #[test]
    fn test_lambda_kv_set_and_get_raw() {
        let kv = create_test_kv();
        kv.set_raw("lambda_a", "key1", vec![1, 2, 3]);

        let value = kv.get_raw("lambda_a", "key1");
        assert_eq!(value, Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_lambda_kv_get_nonexistent_key() {
        let kv = create_test_kv();
        kv.set_raw("lambda_a", "key1", vec![1, 2, 3]);

        let value = kv.get_raw("lambda_a", "nonexistent");
        assert_eq!(value, None);
    }

    #[test]
    fn test_lambda_kv_get_nonexistent_lambda() {
        let kv = create_test_kv();
        kv.set_raw("lambda_a", "key1", vec![1, 2, 3]);

        let value = kv.get_raw("lambda_b", "key1");
        assert_eq!(value, None);
    }

    #[test]
    fn test_lambda_kv_remove_existing() {
        let kv = create_test_kv();
        kv.set_raw("lambda_a", "key1", vec![1, 2, 3]);
        assert_eq!(kv.get_raw("lambda_a", "key1"), Some(vec![1, 2, 3]));

        kv.remove("lambda_a", "key1");
        assert_eq!(kv.get_raw("lambda_a", "key1"), None);
    }

    #[test]
    fn test_lambda_kv_remove_nonexistent() {
        let kv = create_test_kv();
        kv.remove("lambda_a", "nonexistent"); // should not panic
    }

    #[test]
    fn test_lambda_kv_remove_from_nonexistent_lambda() {
        let kv = create_test_kv();
        kv.set_raw("lambda_a", "key1", vec![1, 2, 3]);
        kv.remove("lambda_b", "key1"); // different lambda, should not affect lambda_a
        assert_eq!(kv.get_raw("lambda_a", "key1"), Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_lambda_kv_multiple_lambdas_isolated() {
        let kv = create_test_kv();
        kv.set_raw("lambda_a", "key1", vec![1]);
        kv.set_raw("lambda_b", "key1", vec![2]);
        kv.set_raw("lambda_a", "key2", vec![3]);

        assert_eq!(kv.get_raw("lambda_a", "key1"), Some(vec![1]));
        assert_eq!(kv.get_raw("lambda_b", "key1"), Some(vec![2]));
        assert_eq!(kv.get_raw("lambda_a", "key2"), Some(vec![3]));
        assert_eq!(kv.get_raw("lambda_b", "key2"), None);
    }

    #[test]
    fn test_lambda_kv_same_key_overwrites() {
        let kv = create_test_kv();
        kv.set_raw("lambda_a", "key1", vec![1, 2, 3]);
        kv.set_raw("lambda_a", "key1", vec![4, 5, 6]);

        let value = kv.get_raw("lambda_a", "key1");
        assert_eq!(value, Some(vec![4, 5, 6]));
    }

    // =============================================================================
    // LambdaKV serialization tests (set/get with typed values)
    // =============================================================================

    #[test]
    fn test_lambda_kv_set_get_string() {
        let kv = create_test_kv();
        kv.set("lambda_a", "name", &"Alice");

        let value: Option<String> = kv.get("lambda_a", "name");
        assert_eq!(value, Some("Alice".to_string()));
    }

    #[test]
    fn test_lambda_kv_set_get_u64() {
        let kv = create_test_kv();
        kv.set("lambda_a", "count", &42u64);

        let value: Option<u64> = kv.get("lambda_a", "count");
        assert_eq!(value, Some(42));
    }

    #[test]
    fn test_lambda_kv_set_get_struct() {
        let kv = create_test_kv();

        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct UserData {
            name: String,
            age: u32,
        }

        let user = UserData {
            name: "Bob".to_string(),
            age: 30,
        };
        kv.set("lambda_a", "user", &user);

        let retrieved: Option<UserData> = kv.get("lambda_a", "user");
        assert_eq!(retrieved, Some(user));
    }

    // =============================================================================
    // LambdaKVContext permission tests
    // =============================================================================

    #[test]
    fn test_lambda_kv_context_can_read_self() {
        let kv = Arc::new(create_test_kv());
        let ctx = LambdaKVContext::new(kv, "my_lambda", vec![]);

        assert!(ctx.can_read("my_lambda"));
    }

    #[test]
    fn test_lambda_kv_context_can_read_allowed() {
        let kv = Arc::new(create_test_kv());
        let ctx = LambdaKVContext::new(
            kv,
            "my_lambda",
            vec!["lambda_a".to_string(), "lambda_b".to_string()],
        );

        assert!(ctx.can_read("lambda_a"));
        assert!(ctx.can_read("lambda_b"));
    }

    #[test]
    fn test_lambda_kv_context_cannot_read_unlisted() {
        let kv = Arc::new(create_test_kv());
        let ctx = LambdaKVContext::new(kv, "my_lambda", vec!["lambda_a".to_string()]);

        assert!(!ctx.can_read("lambda_b"));
        assert!(!ctx.can_read("other_lambda"));
    }

    #[test]
    fn test_lambda_kv_context_clone_is_independent() {
        let kv = Arc::new(create_test_kv());
        let ctx1 = LambdaKVContext::new(kv.clone(), "lambda_a", vec![]);
        let ctx2 = ctx1.clone();

        // Both should work independently
        assert_eq!(ctx1.lambda_name, "lambda_a");
        assert_eq!(ctx2.lambda_name, "lambda_a");
    }

    // =============================================================================
    // LambdaKV concurrency safety (Send + Sync)
    // =============================================================================

    #[test]
    fn test_lambda_kv_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<LambdaKV>();
    }

    #[test]
    fn test_lambda_kv_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<LambdaKV>();
    }

    #[test]
    fn test_lambda_kv_context_is_send() {
        fn assert_send<T: Send>() {}
        let kv = Arc::new(create_test_kv());
        let _ctx = LambdaKVContext::new(kv, "test", vec![]);
        assert_send::<LambdaKVContext>();
    }

    #[test]
    fn test_lambda_kv_context_is_sync() {
        fn assert_sync<T: Sync>() {}
        let kv = Arc::new(create_test_kv());
        let _ctx = LambdaKVContext::new(kv, "test", vec![]);
        assert_sync::<LambdaKVContext>();
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use extism::{Manifest, Plugin, Wasm};
    use std::sync::Arc;

    // WASM file for test-kv lambda
    const TEST_KV_WASM: &[u8] =
        include_bytes!("../../../target/wasm32-unknown-unknown/release/test_kv.wasm");

    fn create_test_kv() -> LambdaKV {
        LambdaKV::new()
    }

    fn run_lambda_with_kv<F>(
        kv: Arc<LambdaKV>,
        lambda_name: &str,
        allowed_kv_read: Vec<String>,
        f: F,
    ) where
        F: FnOnce(&mut Plugin),
    {
        let functions = kv_functions(kv, lambda_name, allowed_kv_read);

        let manifest = Manifest::new([Wasm::data(TEST_KV_WASM)]);
        let mut lambda = Plugin::new(manifest, functions, true).unwrap();
        f(&mut lambda);
    }

    #[test]
    fn test_integration_kv_set_and_get() {
        let kv = Arc::new(create_test_kv());

        run_lambda_with_kv(kv, "test-lambda", vec![], |lambda: &mut Plugin| {
            // First call test_kv_set - this exercises host_kv_set
            let _set_result: String = lambda.call("test_kv_set", "").unwrap();
            // Then call test_kv_get - this exercises host_kv_get
            let get_result: String = lambda.call("test_kv_get", "").unwrap();
            // Result is JSON like {"success":true,"message":"test_value_from_lambda"}
            assert!(get_result.contains("success"));
            assert!(get_result.contains("test_value_from_lambda"));
        });
    }

    #[test]
    fn test_integration_kv_remove() {
        let kv = Arc::new(create_test_kv());

        run_lambda_with_kv(kv, "test-lambda", vec![], |lambda: &mut Plugin| {
            // Set a value first
            let _set_result: String = lambda.call("test_kv_set", "").unwrap();
            // Verify it was set
            let get_result: String = lambda.call("test_kv_get", "").unwrap();
            assert!(get_result.contains("success"));
            // Remove it
            let remove_result: String = lambda.call("test_kv_remove", "").unwrap();
            assert!(remove_result.contains("success"));
        });
    }

    #[test]
    fn test_integration_kv_list_readable() {
        let kv = Arc::new(create_test_kv());

        run_lambda_with_kv(
            kv,
            "my-lambda",
            vec!["other-lambda".to_string()],
            |lambda: &mut Plugin| {
                // Call test_kv_list_readable
                let result: String = lambda.call("test_kv_list_readable", "").unwrap();
                let lambdas: Vec<String> = serde_json::from_str(&result).unwrap();
                // Should include self and allowed lambdas
                assert!(lambdas.contains(&"my-lambda".to_string()));
                assert!(lambdas.contains(&"other-lambda".to_string()));
            },
        );
    }

    #[test]
    fn test_integration_kv_list_writable() {
        let kv = Arc::new(create_test_kv());

        run_lambda_with_kv(
            kv,
            "my-lambda",
            vec!["other-lambda".to_string()],
            |lambda: &mut Plugin| {
                // Call test_kv_list_writable
                let result: String = lambda.call("test_kv_list_writable", "").unwrap();
                let lambdas: Vec<String> = serde_json::from_str(&result).unwrap();
                // Should only include self (writes to other lambdas not allowed)
                assert_eq!(lambdas, vec!["my-lambda"]);
            },
        );
    }
}
