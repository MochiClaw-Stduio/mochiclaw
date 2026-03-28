//! Host KV functions for plugins
//!
//! Uses MessagePack encoding for input/output structures.

use extism::{CurrentPlugin, Function, UserData, Val, ValType};
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

/// Shared KV store for all plugin instances
/// Key structure: plugin_name -> key -> msgpack encoded value
pub struct PluginKV {
    store: Arc<std::sync::Mutex<HashMap<String, HashMap<String, Vec<u8>>>>>,
}

impl Clone for PluginKV {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
        }
    }
}

impl PluginKV {
    /// Create a new PluginKV store
    pub fn new() -> Self {
        Self {
            store: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Set a value (auto-serializes to msgpack)
    pub fn set<T: Serialize>(&self, plugin: &str, key: &str, value: &T) {
        let mut buf = Vec::new();
        if value.serialize(&mut Serializer::new(&mut buf)).is_err() {
            tracing::error!("kv serialize error");
            return;
        }

        let mut store = self.store.lock().unwrap();
        store
            .entry(plugin.to_string())
            .or_default()
            .insert(key.to_string(), buf);
    }

    /// Get a value (auto-deserializes from msgpack)
    pub fn get<T: for<'de> Deserialize<'de>>(&self, plugin: &str, key: &str) -> Option<T> {
        let store = self.store.lock().unwrap();
        let bytes = store.get(plugin)?.get(key)?;

        T::deserialize(&mut Deserializer::new(Cursor::new(bytes))).ok()
    }

    /// Set raw bytes (no serialization)
    pub fn set_raw(&self, plugin: &str, key: &str, value: Vec<u8>) {
        let mut store = self.store.lock().unwrap();
        store
            .entry(plugin.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }

    /// Get raw bytes (no deserialization)
    pub fn get_raw(&self, plugin: &str, key: &str) -> Option<Vec<u8>> {
        let store = self.store.lock().unwrap();
        store.get(plugin)?.get(key).cloned()
    }

    /// Remove a value
    pub fn remove(&self, plugin: &str, key: &str) {
        let mut store = self.store.lock().unwrap();
        if let Some(m) = store.get_mut(plugin) {
            m.remove(key);
        }
    }
}

impl Default for PluginKV {
    fn default() -> Self {
        Self::new()
    }
}

/// KV input structures for host functions (MessagePack encoded)
#[derive(Serialize, Deserialize)]
pub struct KVGetInput {
    /// Target plugin to read from (optional, defaults to own plugin if empty)
    pub plugin: Option<String>,
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

/// KV access context bound to a plugin
/// plugin_name: the plugin's own name (for write operations and self-read)
/// allowed_kv_read: list of other plugin names this plugin can read from
pub struct PluginKVContext {
    kv: Arc<PluginKV>,
    plugin_name: String,
    allowed_kv_read: Vec<String>,
}

impl PluginKVContext {
    pub fn new(kv: Arc<PluginKV>, plugin_name: &str, allowed_kv_read: Vec<String>) -> Self {
        Self {
            kv,
            plugin_name: plugin_name.to_string(),
            allowed_kv_read,
        }
    }

    /// Check if reading from target_plugin is allowed
    fn can_read(&self, target_plugin: &str) -> bool {
        target_plugin == self.plugin_name || self.allowed_kv_read.contains(&target_plugin.to_string())
    }
}

impl Clone for PluginKVContext {
    fn clone(&self) -> Self {
        Self {
            kv: self.kv.clone(),
            plugin_name: self.plugin_name.clone(),
            allowed_kv_read: self.allowed_kv_read.clone(),
        }
    }
}

// Manual Send + Sync impls needed for UserData
unsafe impl Send for PluginKVContext {}
unsafe impl Sync for PluginKVContext {}

/// Create all KV host functions bound to a specific plugin
pub fn kv_functions(
    kv: Arc<PluginKV>,
    plugin_name: &str,
    allowed_kv_read: Vec<String>,
) -> Vec<Function> {
    let ctx = PluginKVContext::new(kv, plugin_name, allowed_kv_read);
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
/// Input: MessagePack encoded KVGetInput (plugin + key)
/// Output: MessagePack encoded Option<Vec<u8>> (None if not found or not allowed)
/// Permission: own plugin always allowed, others require declared permission
pub fn kv_get_fn(ctx: PluginKVContext) -> Function {
    Function::new(
        "host_kv_get",
        [ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<PluginKVContext>| {
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

            // Read input from plugin memory
            let input_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            if input_offset == 0 {
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            let handle = match plugin.memory_handle(input_offset) {
                Some(h) => h,
                None => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            let bytes = match plugin.memory_bytes(handle) {
                Ok(b) => b.to_vec(),
                Err(_) => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            // Decode input
            let input: KVGetInput = match KVGetInput::deserialize(&mut Deserializer::new(Cursor::new(&bytes))) {
                Ok(inp) => inp,
                Err(_) => {
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };

            // Default to own plugin if not specified
            let target_plugin = input.plugin.as_ref()
                .filter(|p| !p.is_empty())
                .map(|p| p.as_str())
                .unwrap_or(&ctx_guard.plugin_name);

            // Permission check: can only read from own plugin or allowed plugins
            if !ctx_guard.can_read(target_plugin) {
                tracing::warn!(
                    "kv get denied: plugin '{}' tried to read from '{}' without permission",
                    ctx_guard.plugin_name,
                    target_plugin
                );
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            // Get raw value using PluginKV methods (handles locking internally)
            let value = ctx_guard.kv.get_raw(target_plugin, &input.key);

            // Encode output as MessagePack Option<Vec<u8>>
            let output: Option<Vec<u8>> = value;
            let mut output_buf = Vec::new();
            if output.serialize(&mut Serializer::new(&mut output_buf)).is_err() {
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            if let Err(_) = plugin.memory_set_val(&mut outputs[0], &output_buf) {
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
/// Permission: always writes to own plugin KV only
pub fn kv_set_fn(ctx: PluginKVContext) -> Function {
    Function::new(
        "host_kv_set",
        [ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<PluginKVContext>| {
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

            // Read input from plugin memory
            let input_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            if input_offset == 0 {
                outputs[0] = Val::I64(-1);
                return Ok(());
            }

            let handle = match plugin.memory_handle(input_offset) {
                Some(h) => h,
                None => {
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            let bytes = match plugin.memory_bytes(handle) {
                Ok(b) => b.to_vec(),
                Err(_) => {
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            // Decode input
            let input: KVSetInput = match KVSetInput::deserialize(&mut Deserializer::new(Cursor::new(&bytes))) {
                Ok(inp) => inp,
                Err(_) => {
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            // Always writes to own KV
            ctx_guard.kv.set_raw(&ctx_guard.plugin_name, &input.key, input.value);

            outputs[0] = Val::I64(0);
            Ok(())
        },
    )
}

/// host_kv_remove: remove a value from KV store
///
/// Input: MessagePack encoded KVRemoveInput
/// Output: i64 (0 = success, -1 = not found)
/// Permission: always removes from own plugin KV only
pub fn kv_remove_fn(ctx: PluginKVContext) -> Function {
    Function::new(
        "host_kv_remove",
        [ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<PluginKVContext>| {
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

            // Read input from plugin memory
            let input_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            if input_offset == 0 {
                outputs[0] = Val::I64(-1);
                return Ok(());
            }

            let handle = match plugin.memory_handle(input_offset) {
                Some(h) => h,
                None => {
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            let bytes = match plugin.memory_bytes(handle) {
                Ok(b) => b.to_vec(),
                Err(_) => {
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            // Decode input
            let input: KVRemoveInput = match KVRemoveInput::deserialize(&mut Deserializer::new(Cursor::new(&bytes))) {
                Ok(inp) => inp,
                Err(_) => {
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };

            // Always removes from own KV
            ctx_guard.kv.remove(&ctx_guard.plugin_name, &input.key);

            outputs[0] = Val::I64(0);
            Ok(())
        },
    )
}

/// host_kv_list_readable: list plugins this plugin can read from
///
/// Input: None
/// Output: MessagePack encoded Vec<String> (includes self)
pub fn kv_list_readable_fn(ctx: PluginKVContext) -> Function {
    Function::new(
        "host_kv_list_readable",
        [],
        [ValType::I64],
        UserData::new(ctx),
        |_plugin: &mut CurrentPlugin,
         _inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<PluginKVContext>| {
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
            let mut readable = vec![ctx_guard.plugin_name.clone()];
            readable.extend(ctx_guard.allowed_kv_read.clone());

            let mut output_buf = Vec::new();
            if readable.serialize(&mut Serializer::new(&mut output_buf)).is_err() {
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            if let Err(_) = _plugin.memory_set_val(&mut outputs[0], &output_buf) {
                outputs[0] = Val::I64(0);
            }
            Ok(())
        },
    )
}

/// host_kv_list_writable: list plugins this plugin can write to
///
/// Input: None
/// Output: MessagePack encoded Vec<String> (currently just self)
pub fn kv_list_writable_fn(ctx: PluginKVContext) -> Function {
    Function::new(
        "host_kv_list_writable",
        [],
        [ValType::I64],
        UserData::new(ctx),
        |_plugin: &mut CurrentPlugin,
         _inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<PluginKVContext>| {
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

            // Writable = only self (write to other plugins not allowed)
            let writable = vec![ctx_guard.plugin_name.clone()];

            let mut output_buf = Vec::new();
            if writable.serialize(&mut Serializer::new(&mut output_buf)).is_err() {
                outputs[0] = Val::I64(0);
                return Ok(());
            }

            if let Err(_) = _plugin.memory_set_val(&mut outputs[0], &output_buf) {
                outputs[0] = Val::I64(0);
            }
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_kv() -> PluginKV {
        PluginKV::new()
    }

    // =============================================================================
    // PluginKV basic tests
    // =============================================================================

    #[test]
    fn test_plugin_kv_new_is_empty() {
        let kv = create_test_kv();
        assert!(kv.get_raw("plugin_a", "key").is_none());
    }

    #[test]
    fn test_plugin_kv_set_and_get_raw() {
        let kv = create_test_kv();
        kv.set_raw("plugin_a", "key1", vec![1, 2, 3]);

        let value = kv.get_raw("plugin_a", "key1");
        assert_eq!(value, Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_plugin_kv_get_nonexistent_key() {
        let kv = create_test_kv();
        kv.set_raw("plugin_a", "key1", vec![1, 2, 3]);

        let value = kv.get_raw("plugin_a", "nonexistent");
        assert_eq!(value, None);
    }

    #[test]
    fn test_plugin_kv_get_nonexistent_plugin() {
        let kv = create_test_kv();
        kv.set_raw("plugin_a", "key1", vec![1, 2, 3]);

        let value = kv.get_raw("plugin_b", "key1");
        assert_eq!(value, None);
    }

    #[test]
    fn test_plugin_kv_remove_existing() {
        let kv = create_test_kv();
        kv.set_raw("plugin_a", "key1", vec![1, 2, 3]);
        assert_eq!(kv.get_raw("plugin_a", "key1"), Some(vec![1, 2, 3]));

        kv.remove("plugin_a", "key1");
        assert_eq!(kv.get_raw("plugin_a", "key1"), None);
    }

    #[test]
    fn test_plugin_kv_remove_nonexistent() {
        let kv = create_test_kv();
        kv.remove("plugin_a", "nonexistent"); // should not panic
    }

    #[test]
    fn test_plugin_kv_remove_from_nonexistent_plugin() {
        let kv = create_test_kv();
        kv.set_raw("plugin_a", "key1", vec![1, 2, 3]);
        kv.remove("plugin_b", "key1"); // different plugin, should not affect plugin_a
        assert_eq!(kv.get_raw("plugin_a", "key1"), Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_plugin_kv_multiple_plugins_isolated() {
        let kv = create_test_kv();
        kv.set_raw("plugin_a", "key1", vec![1]);
        kv.set_raw("plugin_b", "key1", vec![2]);
        kv.set_raw("plugin_a", "key2", vec![3]);

        assert_eq!(kv.get_raw("plugin_a", "key1"), Some(vec![1]));
        assert_eq!(kv.get_raw("plugin_b", "key1"), Some(vec![2]));
        assert_eq!(kv.get_raw("plugin_a", "key2"), Some(vec![3]));
        assert_eq!(kv.get_raw("plugin_b", "key2"), None);
    }

    #[test]
    fn test_plugin_kv_same_key_overwrites() {
        let kv = create_test_kv();
        kv.set_raw("plugin_a", "key1", vec![1, 2, 3]);
        kv.set_raw("plugin_a", "key1", vec![4, 5, 6]);

        let value = kv.get_raw("plugin_a", "key1");
        assert_eq!(value, Some(vec![4, 5, 6]));
    }

    // =============================================================================
    // PluginKV serialization tests (set/get with typed values)
    // =============================================================================

    #[test]
    fn test_plugin_kv_set_get_string() {
        let kv = create_test_kv();
        kv.set("plugin_a", "name", &"Alice");

        let value: Option<String> = kv.get("plugin_a", "name");
        assert_eq!(value, Some("Alice".to_string()));
    }

    #[test]
    fn test_plugin_kv_set_get_u64() {
        let kv = create_test_kv();
        kv.set("plugin_a", "count", &42u64);

        let value: Option<u64> = kv.get("plugin_a", "count");
        assert_eq!(value, Some(42));
    }

    #[test]
    fn test_plugin_kv_set_get_struct() {
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
        kv.set("plugin_a", "user", &user);

        let retrieved: Option<UserData> = kv.get("plugin_a", "user");
        assert_eq!(retrieved, Some(user));
    }

    // =============================================================================
    // PluginKVContext permission tests
    // =============================================================================

    #[test]
    fn test_plugin_kv_context_can_read_self() {
        let kv = Arc::new(create_test_kv());
        let ctx = PluginKVContext::new(kv, "my_plugin", vec![]);

        assert!(ctx.can_read("my_plugin"));
    }

    #[test]
    fn test_plugin_kv_context_can_read_allowed() {
        let kv = Arc::new(create_test_kv());
        let ctx = PluginKVContext::new(
            kv,
            "my_plugin",
            vec!["plugin_a".to_string(), "plugin_b".to_string()],
        );

        assert!(ctx.can_read("plugin_a"));
        assert!(ctx.can_read("plugin_b"));
    }

    #[test]
    fn test_plugin_kv_context_cannot_read_unlisted() {
        let kv = Arc::new(create_test_kv());
        let ctx = PluginKVContext::new(kv, "my_plugin", vec!["plugin_a".to_string()]);

        assert!(!ctx.can_read("plugin_b"));
        assert!(!ctx.can_read("other_plugin"));
    }

    #[test]
    fn test_plugin_kv_context_clone_is_independent() {
        let kv = Arc::new(create_test_kv());
        let ctx1 = PluginKVContext::new(kv.clone(), "plugin_a", vec![]);
        let ctx2 = ctx1.clone();

        // Both should work independently
        assert_eq!(ctx1.plugin_name, "plugin_a");
        assert_eq!(ctx2.plugin_name, "plugin_a");
    }

    // =============================================================================
    // PluginKV concurrency safety (Send + Sync)
    // =============================================================================

    #[test]
    fn test_plugin_kv_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<PluginKV>();
    }

    #[test]
    fn test_plugin_kv_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<PluginKV>();
    }

    #[test]
    fn test_plugin_kv_context_is_send() {
        fn assert_send<T: Send>() {}
        let kv = Arc::new(create_test_kv());
        let _ctx = PluginKVContext::new(kv, "test", vec![]);
        assert_send::<PluginKVContext>();
    }

    #[test]
    fn test_plugin_kv_context_is_sync() {
        fn assert_sync<T: Sync>() {}
        let kv = Arc::new(create_test_kv());
        let _ctx = PluginKVContext::new(kv, "test", vec![]);
        assert_sync::<PluginKVContext>();
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use extism::{Plugin, Manifest, Wasm};
    use std::sync::Arc;

    // WASM file for test-kv plugin
    const TEST_KV_WASM: &[u8] = include_bytes!(
        "../../../target/wasm32-unknown-unknown/release/test_kv.wasm"
    );

    fn create_test_kv() -> PluginKV {
        PluginKV::new()
    }

    fn run_plugin_with_kv<F>(kv: Arc<PluginKV>, plugin_name: &str, allowed_kv_read: Vec<String>, f: F)
    where
        F: FnOnce(&mut Plugin),
    {
        let functions = kv_functions(kv, plugin_name, allowed_kv_read);

        let manifest = Manifest::new([Wasm::data(TEST_KV_WASM)]);
        let mut plugin = Plugin::new(manifest, functions, true).unwrap();
        f(&mut plugin);
    }

    #[test]
    fn test_integration_kv_set_and_get() {
        let kv = Arc::new(create_test_kv());

        run_plugin_with_kv(kv, "test-plugin", vec![], |plugin: &mut Plugin| {
            // First call test_kv_set - this exercises host_kv_set
            let _set_result: String = plugin.call("test_kv_set", "").unwrap();
            // Then call test_kv_get - this exercises host_kv_get
            let get_result: String = plugin.call("test_kv_get", "").unwrap();
            // Result is JSON like {"success":true,"message":"test_value_from_plugin"}
            assert!(get_result.contains("success"));
            assert!(get_result.contains("test_value_from_plugin"));
        });
    }

    #[test]
    fn test_integration_kv_remove() {
        let kv = Arc::new(create_test_kv());

        run_plugin_with_kv(kv, "test-plugin", vec![], |plugin: &mut Plugin| {
            // Set a value first
            let _set_result: String = plugin.call("test_kv_set", "").unwrap();
            // Verify it was set
            let get_result: String = plugin.call("test_kv_get", "").unwrap();
            assert!(get_result.contains("success"));
            // Remove it
            let remove_result: String = plugin.call("test_kv_remove", "").unwrap();
            assert!(remove_result.contains("success"));
        });
    }

    #[test]
    fn test_integration_kv_list_readable() {
        let kv = Arc::new(create_test_kv());

        run_plugin_with_kv(kv, "my-plugin", vec!["other-plugin".to_string()], |plugin: &mut Plugin| {
            // Call test_kv_list_readable
            let result: String = plugin.call("test_kv_list_readable", "").unwrap();
            let plugins: Vec<String> = serde_json::from_str(&result).unwrap();
            // Should include self and allowed plugins
            assert!(plugins.contains(&"my-plugin".to_string()));
            assert!(plugins.contains(&"other-plugin".to_string()));
        });
    }

    #[test]
    fn test_integration_kv_list_writable() {
        let kv = Arc::new(create_test_kv());

        run_plugin_with_kv(kv, "my-plugin", vec!["other-plugin".to_string()], |plugin: &mut Plugin| {
            // Call test_kv_list_writable
            let result: String = plugin.call("test_kv_list_writable", "").unwrap();
            let plugins: Vec<String> = serde_json::from_str(&result).unwrap();
            // Should only include self (writes to other plugins not allowed)
            assert_eq!(plugins, vec!["my-plugin"]);
        });
    }
}
