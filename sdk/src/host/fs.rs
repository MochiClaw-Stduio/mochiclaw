//! Host FS functions - safe wrappers for lambda use
//!
//! Provides typed access to filesystem operations from lambdas.

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

// Declare external host functions for FS operations (provided by mochiclaw-lambda)
// Note: we use raw FFI instead of #[host_fn] to avoid extism bug with non-pointer return types
#[link(wasm_import_module = "extism:host/user")]
unsafe extern "C" {
    /// Read a file
    ///
    /// Input: MessagePack encoded FsReadInput
    /// Output: MessagePack encoded String (file content or error message)
    fn host_fs_read(input: u64) -> u64;

    /// Write a file
    ///
    /// Input: MessagePack encoded FsWriteInput
    /// Output: i32 (0 = success, 1 = failed)
    fn host_fs_write(input: u64) -> i32;

    /// Edit a file
    ///
    /// Input: MessagePack encoded FsEditInput
    /// Output: MessagePack encoded String (success message or error)
    fn host_fs_edit(input: u64) -> u64;

    /// List directory contents
    ///
    /// Input: MessagePack encoded FsListInput
    /// Output: MessagePack encoded String (directory listing or error)
    fn host_fs_list(input: u64) -> u64;
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

    // Allocate memory for input
    let input_mem = Memory::from_bytes(&input_bytes)
        .map_err(|e| format!("Failed to allocate memory: {}", e))?;
    let input_offset = input_mem.offset();

    // Call host_fs_read - returns memory offset to MessagePack encoded response
    let output_offset = unsafe { host_fs_read(input_offset) };
    if output_offset == 0 {
        return Err("fs_read failed: invalid response".to_string());
    }

    let output_mem =
        Memory::find(output_offset).ok_or("fs_read failed: could not find output memory")?;
    let output_bytes = output_mem.to_vec();

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

    // Allocate memory for input
    let input_mem = Memory::from_bytes(&input_bytes)
        .map_err(|e| format!("Failed to allocate memory: {}", e))?;
    let input_offset = input_mem.offset();

    // Call host_fs_write - returns 0 for success, 1 for failure (raw i32, not memory offset)
    let result = unsafe { host_fs_write(input_offset) };

    match result {
        0 => Ok(true),
        1 => Err("Write failed".to_string()),
        code => Err(format!("Unexpected return code: {}", code)),
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

    // Allocate memory for input
    let input_mem = Memory::from_bytes(&input_bytes)
        .map_err(|e| format!("Failed to allocate memory: {}", e))?;
    let input_offset = input_mem.offset();

    // Call host_fs_edit - returns memory offset to MessagePack encoded response
    let output_offset = unsafe { host_fs_edit(input_offset) };
    if output_offset == 0 {
        return Err("fs_edit failed: invalid response".to_string());
    }

    let output_mem =
        Memory::find(output_offset).ok_or("fs_edit failed: could not find output memory")?;
    let output_bytes = output_mem.to_vec();

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

    // Allocate memory for input
    let input_mem = Memory::from_bytes(&input_bytes)
        .map_err(|e| format!("Failed to allocate memory: {}", e))?;
    let input_offset = input_mem.offset();

    // Call host_fs_list - returns memory offset to MessagePack encoded response
    let output_offset = unsafe { host_fs_list(input_offset) };
    if output_offset == 0 {
        return Err("fs_list failed: invalid response".to_string());
    }

    let output_mem =
        Memory::find(output_offset).ok_or("fs_list failed: could not find output memory")?;
    let output_bytes = output_mem.to_vec();

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
