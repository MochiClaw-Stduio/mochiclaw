//! Session management for conversation history.
//!
//! Sessions store message history per channel:chat_id key,
//! with JSONL persistence and legal tool-call boundary alignment.

use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// A single message in a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// A tool call structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolFunction,
}

/// A tool function call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

/// Metadata line in JSONL session files
#[derive(Debug, Serialize, Deserialize)]
struct SessionMetadata {
    #[serde(rename = "_type")]
    type_tag: String,
    key: String,
    created_at: String,
    updated_at: String,
    metadata: HashMap<String, serde_json::Value>,
    last_consolidated: usize,
}

/// Session info for listing
#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub key: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub path: String,
}

/// A conversation session
pub struct Session {
    pub key: String,
    pub messages: Vec<Message>,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub last_consolidated: usize,
}

impl Session {
    pub fn new(key: String) -> Self {
        let now = chrono_now();
        Self {
            key,
            messages: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            metadata: HashMap::new(),
            last_consolidated: 0,
        }
    }

    /// Add a message to the session (append-only)
    pub fn add_message(&mut self, role: &str, content: &str) {
        let msg = Message {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            timestamp: Some(chrono_now()),
        };
        self.messages.push(msg);
        self.updated_at = chrono_now();
    }

    /// Add a message with full fields (for tool results etc)
    pub fn add_message_full(
        &mut self,
        role: &str,
        content: &str,
        tool_calls: Option<Vec<ToolCall>>,
        tool_call_id: Option<String>,
        name: Option<String>,
    ) {
        let msg = Message {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls,
            tool_call_id,
            name,
            timestamp: Some(chrono_now()),
        };
        self.messages.push(msg);
        self.updated_at = chrono_now();
    }

    /// Find the index where every tool result has a matching assistant tool_calls.
    /// This ensures we don't slice mid-turn and orphan tool results.
    fn find_legal_start(&self, messages: &[Message]) -> usize {
        let mut declared: HashMap<String, usize> = HashMap::new();
        let mut start = 0;

        for (i, msg) in messages.iter().enumerate() {
            if msg.role == "assistant" {
                if let Some(ref tc) = msg.tool_calls {
                    for call in tc {
                        declared.insert(call.id.clone(), i);
                    }
                }
            } else if msg.role == "tool" {
                if let Some(ref tid) = msg.tool_call_id {
                    if !declared.contains_key(tid) {
                        // Orphan tool result - start after this
                        start = i + 1;
                        declared.clear();
                        // Re-declare from start to i
                        for j in start..=i {
                            if messages[j].role == "assistant" {
                                if let Some(ref tc) = messages[j].tool_calls {
                                    for call in tc {
                                        declared.insert(call.id.clone(), j);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        start
    }

    /// Get message history for LLM input, aligned to legal tool-call boundaries.
    /// Returns messages from last_consolidated onwards, truncated to max_messages.
    pub fn get_history(&self, max_messages: usize) -> Vec<Message> {
        let unconsolidated = if self.last_consolidated >= self.messages.len() {
            &[]
        } else {
            &self.messages[self.last_consolidated..]
        };

        let sliced = if max_messages > 0 && unconsolidated.len() > max_messages {
            &unconsolidated[unconsolidated.len() - max_messages..]
        } else {
            unconsolidated
        };

        // Drop leading non-user messages to avoid starting mid-turn
        let mut result = sliced.to_vec();
        for (i, msg) in result.iter().enumerate() {
            if msg.role == "user" {
                result = result[i..].to_vec();
                break;
            }
        }

        // Align to legal tool-call boundary
        let start = self.find_legal_start(&result);
        if start > 0 {
            result = result[start..].to_vec();
        }

        result
    }

    /// Clear all messages and reset consolidation cursor
    pub fn clear(&mut self) {
        self.messages.clear();
        self.last_consolidated = 0;
        self.updated_at = chrono_now();
    }

    /// Retain only a legal recent suffix, for truncation purposes
    pub fn retain_recent_legal_suffix(&mut self, max_messages: usize) {
        if max_messages == 0 {
            self.clear();
            return;
        }
        if self.messages.len() <= max_messages {
            return;
        }

        let start_idx = self.messages.len() - max_messages;

        // Extend backward to nearest user turn
        let mut actual_start = start_idx;
        while actual_start > 0 && self.messages[actual_start].role != "user" {
            actual_start -= 1;
        }

        let retained = if actual_start > 0 {
            self.messages[actual_start..].to_vec()
        } else {
            self.messages[start_idx..].to_vec()
        };

        // Apply legal start alignment
        let start = self.find_legal_start(&retained);
        let retained = if start > 0 {
            retained[start..].to_vec()
        } else {
            retained
        };

        let dropped = self.messages.len() - retained.len();
        self.messages = retained;
        self.last_consolidated = self.last_consolidated.saturating_sub(dropped);
        self.updated_at = chrono_now();
    }
}

/// Get current UTC time as ISO8601 string
fn chrono_now() -> String {
    // Use time crate for consistent formatting
    let now = time::OffsetDateTime::now_utc();
    now.format(
        &time::format_description::parse("[year]-[month]-[day]T[hour]:[minute]:[second]Z").unwrap(),
    )
    .unwrap_or_else(|_| {
        // Fallback to manual format
        let (year, month, day) = (now.year() as u16, now.month() as u8, now.day());
        let (hour, minute, second) = (now.hour(), now.minute(), now.second());
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            year, month, day, hour, minute, second
        )
    })
}

/// Make a filename safe by replacing unsafe characters
fn safe_filename(key: &str) -> String {
    key.replace(":", "_")
}

/// Session manager for loading, saving, and caching sessions
pub struct SessionManager {
    sessions_dir: PathBuf,
    cache: HashMap<String, Session>,
}

impl SessionManager {
    /// Create a new session manager with the given sessions directory
    pub fn new(sessions_dir: PathBuf) -> Self {
        // Ensure directory exists
        if let Err(e) = fs::create_dir_all(&sessions_dir) {
            tracing::warn!("failed to create sessions_dir {:?}: {}", sessions_dir, e);
        }

        let mut manager = Self {
            sessions_dir,
            cache: HashMap::new(),
        };

        // Pre-load all existing sessions at startup
        manager.load_all();

        manager
    }

    /// Load all existing sessions from disk into cache
    fn load_all(&mut self) {
        let entries = match fs::read_dir(&self.sessions_dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::debug!("load_all: failed to read sessions dir: {}", e);
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }

            // Extract session key from filename (reverse of safe_filename)
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let key = stem.replace("_", ":");
                // Load session if not already in cache
                if !self.cache.contains_key(&key) {
                    if let Some(session) = self.load(&key) {
                        tracing::info!("loaded existing session: {}", key);
                        self.cache.insert(key, session);
                    }
                }
            }
        }
    }

    /// Get or create a session by key
    pub fn get_or_create(&mut self, key: &str) -> &mut Session {
        // Check if session exists in cache
        if self.cache.contains_key(key) {
            return self.cache.get_mut(key).unwrap();
        }

        // Load from disk or create new
        let session = self
            .load(key)
            .unwrap_or_else(|| Session::new(key.to_string()));

        // Insert into cache
        self.cache.insert(key.to_string(), session);

        // Return mutable reference to the newly inserted session
        self.cache.get_mut(key).unwrap()
    }

    /// Get session path for a key
    fn session_path(&self, key: &str) -> PathBuf {
        let safe_key = safe_filename(key);
        self.sessions_dir.join(format!("{}.jsonl", safe_key))
    }

    /// Load a session from disk
    fn load(&mut self, key: &str) -> Option<Session> {
        let path = self.session_path(key);
        if !path.exists() {
            return None;
        }

        let file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("failed to open session file {:?}: {}", path, e);
                return None;
            }
        };

        let reader = BufReader::new(file);
        let mut messages = Vec::new();
        let mut metadata: Option<SessionMetadata> = None;

        for line in reader.lines() {
            match line {
                Ok(l) => {
                    let trimmed = l.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<serde_json::Value>(trimmed) {
                        Ok(val) => {
                            if val.get("_type").and_then(|v| v.as_str()) == Some("metadata") {
                                metadata = serde_json::from_value(val).ok();
                            } else {
                                if let Ok(msg) = serde_json::from_value::<Message>(val) {
                                    messages.push(msg);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::debug!("failed to parse session line: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("failed to read session line: {}", e);
                }
            }
        }

        let meta = metadata?;

        Some(Session {
            key: meta.key,
            messages,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
            metadata: meta.metadata,
            last_consolidated: meta.last_consolidated,
        })
    }

    /// Save a session to disk by key
    pub fn save(&mut self, key: &str) -> Result<(), Error> {
        let session = self
            .cache
            .get(key)
            .ok_or_else(|| Error::Session(format!("session not found: {}", key)))?;

        let path = self.session_path(&session.key);

        let file = fs::File::create(&path).map_err(|e| {
            Error::Session(format!("failed to create session file {:?}: {}", path, e))
        })?;

        let mut writer = std::io::BufWriter::new(file);

        // Write metadata line
        let meta = SessionMetadata {
            type_tag: "metadata".to_string(),
            key: session.key.clone(),
            created_at: session.created_at.clone(),
            updated_at: session.updated_at.clone(),
            metadata: session.metadata.clone(),
            last_consolidated: session.last_consolidated,
        };
        let meta_json = serde_json::to_string(&meta)
            .map_err(|e| Error::Session(format!("failed to serialize metadata: {}", e)))?;
        writeln!(writer, "{}", meta_json)
            .map_err(|e| Error::Session(format!("failed to write metadata: {}", e)))?;

        // Write messages
        for msg in &session.messages {
            let msg_json = serde_json::to_string(msg)
                .map_err(|e| Error::Session(format!("failed to serialize message: {}", e)))?;
            writeln!(writer, "{}", msg_json)
                .map_err(|e| Error::Session(format!("failed to write message: {}", e)))?;
        }

        writer
            .flush()
            .map_err(|e| Error::Session(format!("failed to flush writer: {}", e)))?;

        Ok(())
    }

    /// Remove a session from the cache
    pub fn invalidate(&mut self, key: &str) {
        self.cache.remove(key);
    }

    /// List all sessions on disk
    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        let mut sessions = Vec::new();

        let entries = match fs::read_dir(&self.sessions_dir) {
            Ok(e) => e,
            Err(_) => return sessions,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }

            if let Ok(file) = fs::File::open(&path) {
                let mut reader = BufReader::new(file);
                let mut first_line = String::new();
                if reader.read_line(&mut first_line).is_ok() {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(first_line.trim()) {
                        if val.get("_type").and_then(|v| v.as_str()) == Some("metadata") {
                            let key = val
                                .get("key")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| {
                                    path.file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("")
                                        .replace("_", ":")
                                });

                            sessions.push(SessionInfo {
                                key,
                                created_at: val
                                    .get("created_at")
                                    .and_then(|v| v.as_str().map(String::from)),
                                updated_at: val
                                    .get("updated_at")
                                    .and_then(|v| v.as_str().map(String::from)),
                                path: path.to_string_lossy().to_string(),
                            });
                        }
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let session = Session::new("test:123".to_string());
        assert_eq!(session.key, "test:123");
        assert!(session.messages.is_empty());
        assert_eq!(session.last_consolidated, 0);
    }

    #[test]
    fn test_add_message() {
        let mut session = Session::new("test:123".to_string());
        session.add_message("user", "Hello");
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, "user");
        assert_eq!(session.messages[0].content, "Hello");
    }

    #[test]
    fn test_get_history_empty() {
        let session = Session::new("test:123".to_string());
        let history = session.get_history(100);
        assert!(history.is_empty());
    }

    #[test]
    fn test_get_history_with_messages() {
        let mut session = Session::new("test:123".to_string());
        session.add_message("user", "Hello");
        session.add_message("assistant", "Hi there!");
        session.add_message("user", "How are you?");

        let history = session.get_history(10);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].content, "Hello");
        assert_eq!(history[1].content, "Hi there!");
        assert_eq!(history[2].content, "How are you?");
    }

    #[test]
    fn test_get_history_max_messages() {
        let mut session = Session::new("test:123".to_string());
        for i in 0..10 {
            session.add_message("user", &format!("Message {}", i));
        }

        let history = session.get_history(3);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].content, "Message 7");
    }

    #[test]
    fn test_safe_filename() {
        assert_eq!(safe_filename("telegram:12345"), "telegram_12345");
        assert_eq!(safe_filename("weixin:oABC123"), "weixin_oABC123");
    }
}
