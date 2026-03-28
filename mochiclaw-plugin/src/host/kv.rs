//! Host KV functions for plugins
//!
//! Uses MessagePack encoding for input/output structures.

use extism::{CurrentPlugin, Function, UserData, Val, ValType};
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::sync::Arc;

/// Shared KV store for all plugin instances
/// Key structure: plugin_name -> user_id -> key -> msgpack encoded value
pub struct PluginKV {
    store: Arc<std::sync::Mutex<HashMap<String, HashMap<String, HashMap<String, Vec<u8>>>>>>,
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
    pub fn set<T: Serialize>(&self, plugin: &str, user: &str, key: &str, value: &T) {
        let mut buf = Vec::new();
        if value.serialize(&mut Serializer::new(&mut buf)).is_err() {
            tracing::error!("kv serialize error");
            return;
        }

        let mut store = self.store.lock().unwrap();
        store
            .entry(plugin.to_string())
            .or_default()
            .entry(user.to_string())
            .or_default()
            .insert(key.to_string(), buf);
    }

    /// Get a value (auto-deserializes from msgpack)
    pub fn get<T: for<'de> Deserialize<'de>>(&self, plugin: &str, user: &str, key: &str) -> Option<T> {
        let store = self.store.lock().unwrap();
        let bytes = store.get(plugin)
            .and_then(|m| m.get(user))
            .and_then(|m| m.get(key))?;

        T::deserialize(&mut Deserializer::new(Cursor::new(bytes))).ok()
    }

    /// Set raw bytes (no serialization)
    pub fn set_raw(&self, plugin: &str, user: &str, key: &str, value: Vec<u8>) {
        let mut store = self.store.lock().unwrap();
        store
            .entry(plugin.to_string())
            .or_default()
            .entry(user.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }

    /// Get raw bytes (no deserialization)
    pub fn get_raw(&self, plugin: &str, user: &str, key: &str) -> Option<Vec<u8>> {
        let store = self.store.lock().unwrap();
        store.get(plugin)
            .and_then(|m| m.get(user))
            .and_then(|m| m.get(key))
            .cloned()
    }

    /// Remove a value
    pub fn remove(&self, plugin: &str, user: &str, key: &str) {
        let mut store = self.store.lock().unwrap();
        if let Some(m) = store.get_mut(plugin)
            .and_then(|m| m.get_mut(user)) {
            m.remove(key);
        }
    }
}

impl Default for PluginKV {
    fn default() -> Self {
        Self::new()
    }
}

use std::collections::HashMap;

/// KV input structures for host functions (MessagePack encoded)
#[derive(Serialize, Deserialize)]
pub struct KVGetInput {
    pub plugin: String,
    pub user: String,
    pub key: String,
}

#[derive(Serialize, Deserialize)]
pub struct KVSetInput {
    pub plugin: String,
    pub user: String,
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct KVRemoveInput {
    pub plugin: String,
    pub user: String,
    pub key: String,
}

/// Create all KV host functions: kv_get, kv_set, kv_remove
pub fn kv_functions(kv: Arc<PluginKV>) -> Vec<Function> {
    vec![kv_get_fn(kv.clone()), kv_set_fn(kv.clone()), kv_remove_fn(kv.clone())]
}

/// host_kv_get: get a value from KV store
///
/// Input: MessagePack encoded KVGetInput
/// Output: MessagePack encoded Option<Vec<u8>> (None if not found)
pub fn kv_get_fn(kv: Arc<PluginKV>) -> Function {
    Function::new(
        "host_kv_get",
        [ValType::I64],
        [ValType::I64],
        UserData::new(kv),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<Arc<PluginKV>>| {
            let kv_arc = match user_data.get() {
                Ok(kv) => kv,
                Err(e) => {
                    tracing::error!("kv get failed: {}", e);
                    outputs[0] = Val::I64(0);
                    return Ok(());
                }
            };
            let kv_store = kv_arc.lock()
                .map_err(|e| anyhow::anyhow!("kv lock failed: {}", e))?;

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

            // Get raw value
            let value = kv_store.get_raw(&input.plugin, &input.user, &input.key);

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
pub fn kv_set_fn(kv: Arc<PluginKV>) -> Function {
    Function::new(
        "host_kv_set",
        [ValType::I64],
        [ValType::I64],
        UserData::new(kv),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<Arc<PluginKV>>| {
            let kv_arc = match user_data.get() {
                Ok(kv) => kv,
                Err(e) => {
                    tracing::error!("kv set failed: {}", e);
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };
            let kv_store = kv_arc.lock()
                .map_err(|e| anyhow::anyhow!("kv lock failed: {}", e))?;

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

            kv_store.set_raw(&input.plugin, &input.user, &input.key, input.value);

            outputs[0] = Val::I64(0);
            Ok(())
        },
    )
}

/// host_kv_remove: remove a value from KV store
///
/// Input: MessagePack encoded KVRemoveInput
/// Output: i64 (0 = success, -1 = not found)
pub fn kv_remove_fn(kv: Arc<PluginKV>) -> Function {
    Function::new(
        "host_kv_remove",
        [ValType::I64],
        [ValType::I64],
        UserData::new(kv),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<Arc<PluginKV>>| {
            let kv_arc = match user_data.get() {
                Ok(kv) => kv,
                Err(e) => {
                    tracing::error!("kv remove failed: {}", e);
                    outputs[0] = Val::I64(-1);
                    return Ok(());
                }
            };
            let kv_store = kv_arc.lock()
                .map_err(|e| anyhow::anyhow!("kv lock failed: {}", e))?;

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

            kv_store.remove(&input.plugin, &input.user, &input.key);

            outputs[0] = Val::I64(0);
            Ok(())
        },
    )
}
