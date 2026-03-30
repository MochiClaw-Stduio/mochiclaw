//! Execution History Store - 内存存储用于 durable execution
//!
//! 管理每个 execution_id 的 history HashMap，支持代码重放。

use std::collections::HashMap;
use tokio::sync::RwLock;

/// History Store - 纯内存缓存
pub struct HistoryStore {
    /// 内存缓存: execution_id -> history
    cache: RwLock<HashMap<String, HashMap<String, Vec<u8>>>>,
}

impl HistoryStore {
    /// 创建新的 HistoryStore
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// 获取指定 execution_id 的历史
    pub async fn get(&self, execution_id: &str) -> Option<HashMap<String, Vec<u8>>> {
        let cache = self.cache.read().await;
        cache.get(execution_id).cloned()
    }

    /// 存储指定 execution_id 的完整历史
    pub async fn set(
        &self,
        execution_id: &str,
        history: HashMap<String, Vec<u8>>,
    ) {
        let mut cache = self.cache.write().await;
        cache.insert(execution_id.to_string(), history);
    }

    /// 合并新历史到指定 execution_id
    pub async fn merge(
        &self,
        execution_id: &str,
        new_history: HashMap<String, Vec<u8>>,
    ) {
        let mut cache = self.cache.write().await;
        let entry = cache.entry(execution_id.to_string()).or_insert_with(HashMap::new);
        entry.extend(new_history);
    }

    /// 清除指定 execution_id 的历史
    pub async fn clear(&self, execution_id: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(execution_id);
    }
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_history_store_basic() {
        let store = HistoryStore::new();

        let execution_id = "test-execution-1";

        // 初始应该为空
        assert!(store.get(execution_id).await.is_none());

        // 设置历史
        let mut history = HashMap::new();
        history.insert("step1".to_string(), b"result1".to_vec());
        history.insert("step2".to_string(), b"result2".to_vec());
        store.set(execution_id, history.clone()).await;

        // 应该能获取到
        let retrieved = store.get(execution_id).await.unwrap();
        assert_eq!(retrieved.get("step1"), Some(&b"result1".to_vec()));
        assert_eq!(retrieved.get("step2"), Some(&b"result2".to_vec()));

        // 合并新历史
        let mut new_history = HashMap::new();
        new_history.insert("step3".to_string(), b"result3".to_vec());
        store.merge(execution_id, new_history).await;

        let merged = store.get(execution_id).await.unwrap();
        assert_eq!(merged.len(), 3);

        // 清除历史
        store.clear(execution_id).await;
        assert!(store.get(execution_id).await.is_none());
    }
}
