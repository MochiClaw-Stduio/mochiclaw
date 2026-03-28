//! Host FS functions - safe wrappers for plugin use
//!
//! Provides typed access to filesystem operations from plugins.

use extism_pdk::*;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// FS input structures (MessagePack encoded)
#[derive(Serialize, Deserialize)]
pub struct FsReadInput {
    pub path: String,
    pub workspace: String,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Serialize, Deserialize)]
pub struct FsWriteInput {
    pub path: String,
    pub workspace: String,
    pub content: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct FsEditInput {
    pub path: String,
    pub workspace: String,
    pub old_text: String,
    pub new_text: String,
    pub replace_all: bool,
}

#[derive(Serialize, Deserialize)]
pub struct FsListInput {
    pub path: String,
    pub workspace: String,
    pub recursive: bool,
    pub max_entries: u64,
}

/// Declare external host functions for FS operations (provided by mochiclaw-plugin)
#[host_fn]
extern "ExtismHost" {
    /// Read a file
    ///
    /// Input: MessagePack encoded FsReadInput
    /// Output: MessagePack encoded String (file content or error message)
    fn host_fs_read(input: Vec<u8>) -> Vec<u8>;

    /// Write a file
    ///
    /// Input: MessagePack encoded FsWriteInput
    /// Output: i64 (0 = success, -1 = failed)
    fn host_fs_write(input: Vec<u8>) -> i64;

    /// Edit a file
    ///
    /// Input: MessagePack encoded FsEditInput
    /// Output: MessagePack encoded String (success message or error)
    fn host_fs_edit(input: Vec<u8>) -> Vec<u8>;

    /// List directory contents
    ///
    /// Input: MessagePack encoded FsListInput
    /// Output: MessagePack encoded String (directory listing or error)
    fn host_fs_list(input: Vec<u8>) -> Vec<u8>;
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

/// Read a file
///
/// # Arguments
/// * `path` - file path to read
/// * `workspace` - workspace directory for resolving relative paths
/// * `offset` - line offset to start reading from (1-indexed)
/// * `limit` - maximum number of lines to read
///
/// # Returns
/// * `Ok(String)` - file content or error message
/// * `Err(String)` - error description
pub fn fs_read(path: &str, workspace: &str, offset: u64, limit: u64) -> Result<String, String> {
    let input = FsReadInput {
        path: path.to_string(),
        workspace: workspace.to_string(),
        offset,
        limit,
    };

    let input_bytes = to_msgpack(&input).ok_or("Failed to serialize input")?;

    let output_bytes =
        unsafe { host_fs_read(input_bytes) }.map_err(|e| format!("host_fs_read failed: {}", e))?;

    from_msgpack(&output_bytes).ok_or_else(|| "Failed to deserialize output".to_string())
}

/// Write a file
///
/// # Arguments
/// * `path` - file path to write
/// * `workspace` - workspace directory for resolving relative paths
/// * `content` - content to write
///
/// # Returns
/// * `Ok(true)` on success
/// * `Err(String)` on failure
pub fn fs_write(path: &str, workspace: &str, content: &str) -> Result<bool, String> {
    let input = FsWriteInput {
        path: path.to_string(),
        workspace: workspace.to_string(),
        content: content.as_bytes().to_vec(),
    };

    let input_bytes = to_msgpack(&input).ok_or("Failed to serialize input")?;

    match unsafe { host_fs_write(input_bytes) } {
        Ok(0) => Ok(true),
        Ok(-1) => Err("Write failed".to_string()),
        Ok(code) => Err(format!("Unexpected return code: {}", code)),
        Err(e) => Err(format!("host_fs_write failed: {}", e)),
    }
}

/// Edit a file
///
/// # Arguments
/// * `path` - file path to edit
/// * `workspace` - workspace directory for resolving relative paths
/// * `old_text` - text to find and replace
/// * `new_text` - replacement text
/// * `replace_all` - if true, replace all occurrences
///
/// # Returns
/// * `Ok(String)` - success message
/// * `Err(String)` - error message
pub fn fs_edit(
    path: &str,
    workspace: &str,
    old_text: &str,
    new_text: &str,
    replace_all: bool,
) -> Result<String, String> {
    let input = FsEditInput {
        path: path.to_string(),
        workspace: workspace.to_string(),
        old_text: old_text.to_string(),
        new_text: new_text.to_string(),
        replace_all,
    };

    let input_bytes = to_msgpack(&input).ok_or("Failed to serialize input")?;

    let output_bytes =
        unsafe { host_fs_edit(input_bytes) }.map_err(|e| format!("host_fs_edit failed: {}", e))?;

    from_msgpack(&output_bytes).ok_or_else(|| "Failed to deserialize output".to_string())
}

/// List directory contents
///
/// # Arguments
/// * `path` - directory path to list
/// * `workspace` - workspace directory for resolving relative paths
/// * `recursive` - if true, list recursively
/// * `max_entries` - maximum number of entries to return
///
/// # Returns
/// * `Ok(String)` - directory listing or error message
/// * `Err(String)` - error description
pub fn fs_list(
    path: &str,
    workspace: &str,
    recursive: bool,
    max_entries: u64,
) -> Result<String, String> {
    let input = FsListInput {
        path: path.to_string(),
        workspace: workspace.to_string(),
        recursive,
        max_entries,
    };

    let input_bytes = to_msgpack(&input).ok_or("Failed to serialize input")?;

    let output_bytes =
        unsafe { host_fs_list(input_bytes) }.map_err(|e| format!("host_fs_list failed: {}", e))?;

    from_msgpack(&output_bytes).ok_or_else(|| "Failed to deserialize output".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fs_read_input_serialization() {
        let input = FsReadInput {
            path: "test.txt".to_string(),
            workspace: "/workspace".to_string(),
            offset: 1,
            limit: 100,
        };
        let serialized = to_msgpack(&input).unwrap();
        let deserialized: FsReadInput = from_msgpack(&serialized).unwrap();
        assert_eq!(input.path, deserialized.path);
        assert_eq!(input.workspace, deserialized.workspace);
        assert_eq!(input.offset, deserialized.offset);
        assert_eq!(input.limit, deserialized.limit);
    }

    #[test]
    fn test_fs_write_input_serialization() {
        let input = FsWriteInput {
            path: "test.txt".to_string(),
            workspace: "/workspace".to_string(),
            content: vec![104, 101, 108, 108, 111], // "hello"
        };
        let serialized = to_msgpack(&input).unwrap();
        let deserialized: FsWriteInput = from_msgpack(&serialized).unwrap();
        assert_eq!(input.path, deserialized.path);
        assert_eq!(input.workspace, deserialized.workspace);
        assert_eq!(input.content, deserialized.content);
    }

    #[test]
    fn test_fs_edit_input_serialization() {
        let input = FsEditInput {
            path: "test.txt".to_string(),
            workspace: "/workspace".to_string(),
            old_text: "hello".to_string(),
            new_text: "world".to_string(),
            replace_all: false,
        };
        let serialized = to_msgpack(&input).unwrap();
        let deserialized: FsEditInput = from_msgpack(&serialized).unwrap();
        assert_eq!(input.path, deserialized.path);
        assert_eq!(input.old_text, deserialized.old_text);
        assert_eq!(input.new_text, deserialized.new_text);
        assert_eq!(input.replace_all, deserialized.replace_all);
    }

    #[test]
    fn test_fs_list_input_serialization() {
        let input = FsListInput {
            path: ".".to_string(),
            workspace: "/workspace".to_string(),
            recursive: false,
            max_entries: 100,
        };
        let serialized = to_msgpack(&input).unwrap();
        let deserialized: FsListInput = from_msgpack(&serialized).unwrap();
        assert_eq!(input.path, deserialized.path);
        assert_eq!(input.recursive, deserialized.recursive);
        assert_eq!(input.max_entries, deserialized.max_entries);
    }
}
