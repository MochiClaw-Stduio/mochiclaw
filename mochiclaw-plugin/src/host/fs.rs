//! Host FS functions for plugins
//!
//! Uses MessagePack encoding for input/output structures.

use extism::{CurrentPlugin, Function, UserData, Val, ValType};
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::{Path, PathBuf};

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

/// FS access context bound to a plugin
pub struct FsContext {
    /// Allowed root directory for filesystem operations (sandbox boundary)
    pub allowed_root: PathBuf,

    // 读权限白名单
    pub read_whitelist: Vec<PathBuf>,
    // 写权限白名单
    pub write_whitelist: Vec<PathBuf>,
    // 读权限黑名单（优先级高于白名单）
    pub read_blacklist: Vec<PathBuf>,
    // 写权限黑名单（优先级高于白名单）
    pub write_blacklist: Vec<PathBuf>,
}

impl FsContext {
    pub fn new(
        allowed_root: PathBuf,
        read_whitelist: Vec<PathBuf>,
        write_whitelist: Vec<PathBuf>,
        read_blacklist: Vec<PathBuf>,
        write_blacklist: Vec<PathBuf>,
    ) -> Self {
        // Canonicalize allowed_root first
        let allowed_root: PathBuf = match allowed_root.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // If canonicalize fails, use as-is but deny all access
                return Self {
                    allowed_root,
                    read_whitelist: Vec::new(),
                    write_whitelist: Vec::new(),
                    read_blacklist: Vec::new(),
                    write_blacklist: Vec::new(),
                };
            }
        };

        let resolve_list = |paths: Vec<PathBuf>| -> Vec<PathBuf> {
            paths
                .into_iter()
                .filter_map(|p| p.canonicalize().ok())
                .collect()
        };

        Self {
            allowed_root: allowed_root.clone(),
            read_whitelist: resolve_list(read_whitelist),
            write_whitelist: resolve_list(write_whitelist),
            read_blacklist: resolve_list(read_blacklist),
            write_blacklist: resolve_list(write_blacklist),
        }
    }

    /// Check if reading from the given resolved path is allowed
    fn can_read(&self, resolved_path: &Path) -> bool {
        // First check if within allowed_root
        if !resolved_path.starts_with(&self.allowed_root) {
            return false;
        }
        // Check blacklist first
        if self.is_in_list(resolved_path, &self.read_blacklist) {
            return false;
        }
        // Then check whitelist - empty whitelist means allow all within allowed_root
        if self.read_whitelist.is_empty() {
            true
        } else {
            self.is_in_list(resolved_path, &self.read_whitelist)
        }
    }

    /// Check if writing to the given resolved path is allowed
    fn can_write(&self, resolved_path: &Path) -> bool {
        // First check if within allowed_root
        if !resolved_path.starts_with(&self.allowed_root) {
            return false;
        }
        // Check write blacklist first
        if self.is_in_list(resolved_path, &self.write_blacklist) {
            return false;
        }
        // Then check write whitelist - empty whitelist means allow all within allowed_root
        if self.write_whitelist.is_empty() {
            true
        } else {
            self.is_in_list(resolved_path, &self.write_whitelist)
        }
    }

    fn is_in_list(&self, resolved_path: &Path, list: &[PathBuf]) -> bool {
        list.iter()
            .any(|p| resolved_path.starts_with(p) || resolved_path == *p)
    }

    /// Resolve a path with workspace context (for write operations).
    /// The file may not exist yet. We verify the final path stays within allowed_root.
    fn resolve_path_for_write(&self, path: &Path, workspace: &Path) -> Result<PathBuf, String> {
        // Resolve workspace relative to allowed_root
        let resolved_workspace = if workspace.as_os_str().is_empty() {
            self.allowed_root.clone()
        } else if workspace.is_absolute() {
            workspace.to_path_buf()
        } else {
            self.allowed_root.join(workspace)
        }
        .canonicalize()
        .map_err(|e| format!("Failed to resolve workspace: {}", e))?;

        // Check workspace is within allowed_root
        if !resolved_workspace.starts_with(&self.allowed_root) {
            return Err(format!(
                "Workspace '{}' is outside allowed root '{}'",
                resolved_workspace.display(),
                self.allowed_root.display()
            ));
        }

        // Resolve path relative to workspace
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else {
            resolved_workspace.join(path)
        };

        // If path exists, canonicalize it to resolve symlinks, etc.
        // If it doesn't exist, try canonicalizing parent; if parent also doesn't exist,
        // use components to build the path (we'll create parent dirs in fs_write)
        let resolved_path = if joined.exists() {
            joined
                .canonicalize()
                .map_err(|e| format!("Failed to resolve path: {}", e))?
        } else if let Some(parent) = joined.parent() {
            if parent.exists() {
                // Parent exists, canonicalize it and append filename
                let canonicalized_parent = parent
                    .canonicalize()
                    .map_err(|e| format!("Failed to resolve parent directory: {}", e))?;
                let filename = joined
                    .file_name()
                    .ok_or_else(|| "Invalid path: no filename".to_string())?;
                canonicalized_parent.join(filename)
            } else {
                // Parent doesn't exist either - we'll create it later in fs_write
                // For now, just normalize the path by resolving .. components
                let mut components = Vec::new();
                for c in joined.components() {
                    match c {
                        std::path::Component::ParentDir => {
                            components.pop();
                        }
                        std::path::Component::Normal(name) => {
                            components.push(name);
                        }
                        _ => {}
                    }
                }
                let mut result = self.allowed_root.clone();
                for c in components {
                    result = result.join(c);
                }
                result
            }
        } else {
            return Err(format!("Invalid path: '{}'", joined.display()));
        };

        // Check final path is within allowed_root
        if !resolved_path.starts_with(&self.allowed_root) {
            return Err(format!(
                "Path '{}' is outside allowed root '{}'",
                resolved_path.display(),
                self.allowed_root.display()
            ));
        }

        Ok(resolved_path)
    }

    /// Resolve a path with workspace context (for read operations).
    /// For reading, the file must already exist.
    fn resolve_path_with_workspace(
        &self,
        path: &Path,
        workspace: &Path,
    ) -> Result<PathBuf, String> {
        // Resolve workspace relative to allowed_root
        let resolved_workspace = if workspace.as_os_str().is_empty() {
            self.allowed_root.clone()
        } else if workspace.is_absolute() {
            workspace.to_path_buf()
        } else {
            self.allowed_root.join(workspace)
        }
        .canonicalize()
        .map_err(|e| format!("Failed to resolve workspace: {}", e))?;

        // Check workspace is within allowed_root
        if !resolved_workspace.starts_with(&self.allowed_root) {
            return Err(format!(
                "Workspace '{}' is outside allowed root '{}'",
                resolved_workspace.display(),
                self.allowed_root.display()
            ));
        }

        // Resolve path relative to workspace
        let resolved_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            resolved_workspace.join(path)
        }
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {}", e))?;

        // Check final path is within allowed_root
        if !resolved_path.starts_with(&self.allowed_root) {
            return Err(format!(
                "Path '{}' is outside allowed root '{}'",
                resolved_path.display(),
                self.allowed_root.display()
            ));
        }

        Ok(resolved_path)
    }
}

impl Clone for FsContext {
    fn clone(&self) -> Self {
        Self {
            allowed_root: self.allowed_root.clone(),
            read_whitelist: self.read_whitelist.clone(),
            write_whitelist: self.write_whitelist.clone(),
            read_blacklist: self.read_blacklist.clone(),
            write_blacklist: self.write_blacklist.clone(),
        }
    }
}

// Manual Send + Sync impls needed for UserData
unsafe impl Send for FsContext {}
unsafe impl Sync for FsContext {}

/// Create all FS host functions bound to a specific context
pub fn fs_functions(ctx: FsContext) -> Vec<Function> {
    vec![
        fs_read_fn(ctx.clone()),
        fs_write_fn(ctx.clone()),
        fs_edit_fn(ctx.clone()),
        fs_list_fn(ctx),
    ]
}

/// Helper: serialize to msgpack
fn to_msgpack<T: Serialize>(value: &T) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    value.serialize(&mut Serializer::new(&mut buf)).ok()?;
    Some(buf)
}

/// Helper: deserialize from msgpack
fn from_msgpack<'a, T: Deserialize<'a>>(buf: &'a [u8]) -> Option<T> {
    T::deserialize(&mut Deserializer::new(Cursor::new(buf))).ok()
}

/// host_fs_read: read a file
///
/// Input: MessagePack encoded FsReadInput
/// Output: MessagePack encoded String (file content or error message)
pub fn fs_read_fn(ctx: FsContext) -> Function {
    Function::new(
        "host_fs_read",
        [ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<FsContext>| {
            let ctx = match user_data.get() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("fs_read failed to get context: {}", e);
                    return Ok(());
                }
            };
            let ctx = match ctx.lock() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("fs_read failed to lock context: {}", e);
                    return Ok(());
                }
            };

            // Read input from plugin memory
            let input_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            if input_offset == 0 {
                let error = to_msgpack(&"Invalid input".to_string()).unwrap_or_default();
                let _ = plugin.memory_set_val(&mut outputs[0], &error);
                return Ok(());
            }

            let handle = match plugin.memory_handle(input_offset) {
                Some(h) => h,
                None => {
                    let error =
                        to_msgpack(&"Invalid memory handle".to_string()).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            let bytes = match plugin.memory_bytes(handle) {
                Ok(b) => b.to_vec(),
                Err(_) => {
                    let error =
                        to_msgpack(&"Failed to read memory".to_string()).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            let input: FsReadInput = match from_msgpack(&bytes) {
                Some(inp) => inp,
                None => {
                    let error =
                        to_msgpack(&"Failed to deserialize input".to_string()).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            // Resolve path with workspace context
            let resolved = match ctx
                .resolve_path_with_workspace(Path::new(&input.path), Path::new(&input.workspace))
            {
                Ok(p) => p,
                Err(e) => {
                    let error = to_msgpack(&format!("Error: {}", e)).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            // Permission check on resolved path
            if !ctx.can_read(&resolved) {
                let error = to_msgpack(&format!("Permission denied: cannot read '{}'", input.path))
                    .unwrap_or_default();
                let _ = plugin.memory_set_val(&mut outputs[0], &error);
                return Ok(());
            }

            // Read file
            let result = read_file_impl(&resolved, input.offset, input.limit);

            let output = to_msgpack(&result).unwrap_or_default();
            let _ = plugin.memory_set_val(&mut outputs[0], &output);
            Ok(())
        },
    )
}

/// host_fs_write: write a file
///
/// Input: MessagePack encoded FsWriteInput
/// Output: i32 (0 = success, 1 = failed)
pub fn fs_write_fn(ctx: FsContext) -> Function {
    Function::new(
        "host_fs_write",
        [ValType::I64],
        [ValType::I32],
        UserData::new(ctx),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<FsContext>| {
            let ctx = match user_data.get() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("fs_write failed to get context: {}", e);
                    outputs[0] = Val::I32(1);
                    return Ok(());
                }
            };
            let ctx = match ctx.lock() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("fs_write failed to lock context: {}", e);
                    outputs[0] = Val::I32(1);
                    return Ok(());
                }
            };

            // Read input from plugin memory
            let input_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            if input_offset == 0 {
                outputs[0] = Val::I32(1);
                return Ok(());
            }

            let handle = match plugin.memory_handle(input_offset) {
                Some(h) => h,
                None => {
                    outputs[0] = Val::I32(1);
                    return Ok(());
                }
            };

            let bytes = match plugin.memory_bytes(handle) {
                Ok(b) => b.to_vec(),
                Err(_) => {
                    outputs[0] = Val::I32(1);
                    return Ok(());
                }
            };

            let input: FsWriteInput = match from_msgpack(&bytes) {
                Some(inp) => inp,
                None => {
                    outputs[0] = Val::I32(1);
                    return Ok(());
                }
            };

            // Resolve path with workspace context (for write, file may not exist yet)
            let resolved = match ctx
                .resolve_path_for_write(Path::new(&input.path), Path::new(&input.workspace))
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(
                        "fs_write path resolution failed: path={}, workspace={}, error={}",
                        input.path,
                        input.workspace,
                        e
                    );
                    outputs[0] = Val::I32(1);
                    return Ok(());
                }
            };

            // Permission check on resolved path
            if !ctx.can_write(&resolved) {
                tracing::warn!("fs_write denied: cannot write '{}'", resolved.display());
                outputs[0] = Val::I32(1);
                return Ok(());
            }

            // Create parent directories if needed
            if let Some(parent) = resolved.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                tracing::error!("fs_write failed to create parent dir: {}", e);
                outputs[0] = Val::I32(1);
                return Ok(());
            }

            // Write file: 0 = success, 1 = failure
            match std::fs::write(&resolved, &input.content) {
                Ok(_) => {
                    tracing::debug!("fs_write success: {}", resolved.display());
                    outputs[0] = Val::I32(0);
                }
                Err(e) => {
                    tracing::error!("fs_write failed: {}", e);
                    outputs[0] = Val::I32(1);
                }
            }
            Ok(())
        },
    )
}

/// host_fs_edit: edit a file
///
/// Input: MessagePack encoded FsEditInput
/// Output: MessagePack encoded String (success message or error)
pub fn fs_edit_fn(ctx: FsContext) -> Function {
    Function::new(
        "host_fs_edit",
        [ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<FsContext>| {
            let ctx = match user_data.get() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("fs_edit failed to get context: {}", e);
                    return Ok(());
                }
            };
            let ctx = match ctx.lock() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("fs_edit failed to lock context: {}", e);
                    return Ok(());
                }
            };

            // Read input from plugin memory
            let input_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            if input_offset == 0 {
                let error = to_msgpack(&"Invalid input".to_string()).unwrap_or_default();
                let _ = plugin.memory_set_val(&mut outputs[0], &error);
                return Ok(());
            }

            let handle = match plugin.memory_handle(input_offset) {
                Some(h) => h,
                None => {
                    let error =
                        to_msgpack(&"Invalid memory handle".to_string()).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            let bytes = match plugin.memory_bytes(handle) {
                Ok(b) => b.to_vec(),
                Err(_) => {
                    let error =
                        to_msgpack(&"Failed to read memory".to_string()).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            let input: FsEditInput = match from_msgpack(&bytes) {
                Some(inp) => inp,
                None => {
                    let error =
                        to_msgpack(&"Failed to deserialize input".to_string()).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            // Resolve path with workspace context
            let resolved = match ctx
                .resolve_path_with_workspace(Path::new(&input.path), Path::new(&input.workspace))
            {
                Ok(p) => p,
                Err(e) => {
                    let error = to_msgpack(&format!("Error: {}", e)).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            // Permission check on resolved path
            if !ctx.can_write(&resolved) {
                let error = to_msgpack(&format!("Permission denied: cannot edit '{}'", input.path))
                    .unwrap_or_default();
                let _ = plugin.memory_set_val(&mut outputs[0], &error);
                return Ok(());
            }

            // Edit file
            let result = edit_file_impl(
                &resolved,
                &input.old_text,
                &input.new_text,
                input.replace_all,
            );

            let output = to_msgpack(&result).unwrap_or_default();
            let _ = plugin.memory_set_val(&mut outputs[0], &output);
            Ok(())
        },
    )
}

/// host_fs_list: list directory contents
///
/// Input: MessagePack encoded FsListInput
/// Output: MessagePack encoded String (directory listing or error)
pub fn fs_list_fn(ctx: FsContext) -> Function {
    Function::new(
        "host_fs_list",
        [ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         user_data: UserData<FsContext>| {
            let ctx = match user_data.get() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("fs_list failed to get context: {}", e);
                    return Ok(());
                }
            };
            let ctx = match ctx.lock() {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("fs_list failed to lock context: {}", e);
                    return Ok(());
                }
            };

            // Read input from plugin memory
            let input_offset = inputs.first().and_then(|v| v.i64()).unwrap_or(0) as u64;
            if input_offset == 0 {
                let error = to_msgpack(&"Invalid input".to_string()).unwrap_or_default();
                let _ = plugin.memory_set_val(&mut outputs[0], &error);
                return Ok(());
            }

            let handle = match plugin.memory_handle(input_offset) {
                Some(h) => h,
                None => {
                    let error =
                        to_msgpack(&"Invalid memory handle".to_string()).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            let bytes = match plugin.memory_bytes(handle) {
                Ok(b) => b.to_vec(),
                Err(_) => {
                    let error =
                        to_msgpack(&"Failed to read memory".to_string()).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            let input: FsListInput = match from_msgpack(&bytes) {
                Some(inp) => inp,
                None => {
                    let error =
                        to_msgpack(&"Failed to deserialize input".to_string()).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            // Resolve path with workspace context
            let resolved = match ctx
                .resolve_path_with_workspace(Path::new(&input.path), Path::new(&input.workspace))
            {
                Ok(p) => p,
                Err(e) => {
                    let error = to_msgpack(&format!("Error: {}", e)).unwrap_or_default();
                    let _ = plugin.memory_set_val(&mut outputs[0], &error);
                    return Ok(());
                }
            };

            // Permission check on resolved path (read permission for listing)
            if !ctx.can_read(&resolved) {
                let error = to_msgpack(&format!("Permission denied: cannot list '{}'", input.path))
                    .unwrap_or_default();
                let _ = plugin.memory_set_val(&mut outputs[0], &error);
                return Ok(());
            }

            // List directory
            let result = list_dir_impl(&resolved, input.recursive, input.max_entries);

            let output = to_msgpack(&result).unwrap_or_default();
            let _ = plugin.memory_set_val(&mut outputs[0], &output);
            Ok(())
        },
    )
}

// ============================================================================
// File operation implementations (mirroring nanobot filesystem.py)
// ============================================================================

const MAX_CHARS: usize = 128_000;
const DEFAULT_MAX_ENTRIES: usize = 200;

const IGNORE_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".coverage",
    "htmlcov",
];

fn read_file_impl(path: &Path, offset: u64, limit: u64) -> String {
    if !path.exists() {
        return format!("Error: File not found: {}", path.display());
    }
    if !path.is_file() {
        return format!("Error: Not a file: {}", path.display());
    }

    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return format!("Error reading file: {}", e),
    };

    if raw.is_empty() {
        return format!("(Empty file: {})", path.display());
    }

    // Detect if it's an image
    if let Some(mime) = detect_image_mime(&raw) {
        return format!(
            "(Image file: {} - MIME type: {}, {} bytes)",
            path.display(),
            mime,
            raw.len()
        );
    }

    let text_content = match String::from_utf8(raw) {
        Ok(s) => s,
        Err(_) => {
            return format!(
                "Error: Cannot read binary file {}. Only UTF-8 text and images are supported.",
                path.display()
            );
        }
    };

    let all_lines: Vec<&str> = text_content.split('\n').collect();
    let total = all_lines.len();

    let offset = if offset < 1 { 1 } else { offset } as usize;
    if offset > total {
        return format!(
            "Error: offset {} is beyond end of file ({} lines)",
            offset, total
        );
    }

    let start = offset - 1;
    let end = std::cmp::min(start + limit as usize, total);
    let numbered: Vec<String> = (start..end)
        .map(|i| format!("{}| {}", start + i + 1, all_lines[i]))
        .collect();
    let mut result = numbered.join("\n");

    // Truncate if result is too large
    if result.len() > MAX_CHARS {
        let mut chars = 0;
        let mut truncated_end = start;
        for (i, line) in numbered.iter().enumerate() {
            chars += line.len() + 1;
            if chars > MAX_CHARS {
                break;
            }
            truncated_end = start + i + 1;
        }
        result = numbered[..(truncated_end - start)].join("\n");
        result += "\n\n(truncated due to size limit)";
    }

    if end < total {
        result += &format!(
            "\n\n(Showing lines {}-{} of {}. Use offset={} to continue.)",
            offset,
            end,
            total,
            end + 1
        );
    } else {
        result += &format!("\n\n(End of file — {} lines total)", total);
    }

    result
}

fn detect_image_mime(data: &[u8]) -> Option<String> {
    if data.len() >= 8 {
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Some("image/png".to_string());
        }
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some("image/jpeg".to_string());
        }
        if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            return Some("image/gif".to_string());
        }
        if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
            return Some("image/webp".to_string());
        }
        if data.starts_with(b"%PDF") {
            return Some("application/pdf".to_string());
        }
    }
    None
}

fn edit_file_impl(path: &Path, old_text: &str, new_text: &str, replace_all: bool) -> String {
    if !path.exists() {
        return format!("Error: File not found: {}", path.display());
    }

    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return format!("Error reading file: {}", e),
    };

    let uses_crlf = raw.contains(&b'\r');
    let content = match String::from_utf8(raw) {
        Ok(c) => c,
        Err(e) => return format!("Error decoding file as UTF-8: {}", e),
    }
    .replace("\r\n", "\n");

    let old_normalized = old_text.replace("\r\n", "\n");
    let new_normalized = new_text.replace("\r\n", "\n");

    let (matched, count) = match find_match(&content, &old_normalized) {
        Ok(m) => m,
        Err(e) => return e,
    };

    if count > 1 && !replace_all {
        return format!(
            "Warning: old_text appears {} times. Provide more context to make it unique, or set replace_all=true.",
            count
        );
    }

    let new_content = if replace_all {
        content.replace(&old_normalized, &new_normalized)
    } else {
        let idx = match content.find(&matched) {
            Some(i) => i,
            None => return "Error: matched text not found".to_string(),
        };
        let mut c = content.clone();
        c.replace_range(idx..idx + matched.len(), &new_normalized);
        c
    };

    let final_content = if uses_crlf {
        new_content.replace("\n", "\r\n")
    } else {
        new_content
    };

    if let Err(e) = std::fs::write(path, final_content.as_bytes()) {
        return format!("Error writing file: {}", e);
    }

    format!("Successfully edited {}", path.display())
}

fn find_match(content: &str, old_text: &str) -> Result<(String, usize), String> {
    if let Some(_idx) = content.find(old_text) {
        let count = content.matches(old_text).count();
        return Ok((old_text.to_string(), count));
    }

    let old_lines: Vec<&str> = old_text.split('\n').collect();
    if old_lines.is_empty() {
        return Err("old_text not found".to_string());
    }

    let stripped_old: Vec<&str> = old_lines.iter().map(|l| l.trim()).collect();
    let content_lines: Vec<&str> = content.split('\n').collect();

    let mut candidates: Vec<String> = Vec::new();
    for i in 0..(content_lines
        .len()
        .saturating_sub(stripped_old.len().saturating_sub(1)))
    {
        let window = &content_lines[i..i + stripped_old.len()];
        let stripped_window: Vec<&str> = window.iter().map(|l| l.trim()).collect();
        if stripped_window == stripped_old {
            candidates.push(window.join("\n"));
        }
    }

    if candidates.is_empty() {
        return Err(not_found_msg(old_text, content));
    }

    Ok((candidates[0].clone(), candidates.len()))
}

fn not_found_msg(old_text: &str, content: &str) -> String {
    use std::cmp::Ordering;

    let old_lines: Vec<&str> = old_text.split('\n').collect();
    let content_lines: Vec<&str> = content.split('\n').collect();
    let window_len = old_lines.len();

    let mut best_ratio = 0.0f64;
    let mut best_start = 0usize;

    for i in 0..content_lines
        .len()
        .saturating_sub(window_len.saturating_sub(1))
    {
        let window = &content_lines[i..i + window_len];
        let ratio = similarity_ratio(&old_lines, window);
        match ratio.partial_cmp(&best_ratio) {
            Some(Ordering::Greater) | None => {
                best_ratio = ratio;
                best_start = i;
            }
            _ => {}
        }
    }

    if best_ratio > 0.5 {
        return format!(
            "Error: old_text not found.\nBest match ({:.0}% similar) at line {}:\n--- old_text (provided)\n+++ file (actual, line {})\n{}",
            best_ratio * 100.0,
            best_start + 1,
            best_start + 1,
            generate_diff(
                &old_lines,
                &content_lines[best_start..best_start + window_len]
            )
        );
    }

    "Error: old_text not found. No similar text found. Verify the file content.".to_string()
}

fn similarity_ratio(a: &[&str], b: &[&str]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let mut matches = 0usize;
    for (i, a_elem) in a.iter().enumerate() {
        if i < b.len() && *a_elem == b[i].trim() {
            matches += 1;
        }
    }

    matches as f64 / (a.len().max(b.len())) as f64
}

fn generate_diff(a: &[&str], b: &[&str]) -> String {
    let mut lines = Vec::new();
    for line in a {
        lines.push(format!("-{}", line));
    }
    for line in b {
        lines.push(format!("+{}", line));
    }
    lines.join("\n")
}

fn list_dir_impl(path: &Path, recursive: bool, max_entries: u64) -> String {
    if !path.exists() {
        return format!("Error: Directory not found: {}", path.display());
    }
    if !path.is_dir() {
        return format!("Error: Not a directory: {}", path.display());
    }

    let mut items: Vec<String> = Vec::new();
    let mut total = 0usize;
    let max_entries = if max_entries == 0 {
        DEFAULT_MAX_ENTRIES
    } else {
        max_entries as usize
    };

    if recursive {
        if let Ok(entries) = walkdir(path, IGNORE_DIRS) {
            total = entries.len();
            for entry in entries.into_iter().take(max_entries) {
                let rel_path = entry.strip_prefix(path).unwrap_or(&entry);
                let name = rel_path.display().to_string();
                if entry.is_dir() {
                    items.push(format!("{}/", name));
                } else {
                    items.push(name);
                }
            }
        }
    } else {
        let mut entries: Vec<_> = match std::fs::read_dir(path) {
            Ok(e) => e.filter_map(|e| e.ok()).collect(),
            Err(e) => return format!("Error reading directory: {}", e),
        };

        entries.sort_by_key(|a| a.file_name());

        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if IGNORE_DIRS.contains(&name.as_str()) {
                continue;
            }
            total += 1;
            if items.len() < max_entries {
                if entry.path().is_dir() {
                    items.push(format!("{}/", name));
                } else {
                    items.push(name);
                }
            }
        }
    }

    if items.is_empty() && total == 0 {
        return format!("Directory {} is empty", path.display());
    }

    let mut result = items.join("\n");
    if total > max_entries {
        result += &format!(
            "\n\n(truncated, showing first {} of {} entries)",
            max_entries, total
        );
    }

    result
}

fn walkdir(base: &Path, ignore: &[&str]) -> Result<Vec<PathBuf>, String> {
    let mut results = Vec::new();
    walkdir_recursive(base, ignore, &mut results)?;
    results.sort();
    Ok(results)
}

fn walkdir_recursive(
    dir: &Path,
    ignore: &[&str],
    results: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Error reading directory: {}", e))?;

    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();

        if entry.path().is_dir() && ignore.contains(&name.as_str()) {
            continue;
        }

        results.push(entry.path());

        if entry.path().is_dir() {
            walkdir_recursive(&entry.path(), ignore, results)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Helper to create a temp directory structure for testing
    fn create_test_dirs() -> (TempDir, PathBuf, PathBuf, PathBuf) {
        let temp = TempDir::new().unwrap();
        // Canonicalize to avoid /tmp -> /private/tmp issues on macOS
        let root = temp.path().canonicalize().unwrap();

        // Create structure: root/src/inner, root/secret.txt
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(root.join("secret.txt"), "secret").unwrap();
        fs::write(src_dir.join("file.rs"), "code").unwrap();

        // Create a symlink pointing outside: root/link_to_secret -> /etc/passwd or similar
        #[cfg(unix)]
        let symlink_outside = root.join("link_to_parent");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&root, &symlink_outside).unwrap();

        #[cfg(windows)]
        let symlink_outside = {
            // On Windows, creating symlinks requires admin, so we skip this part
            root.join("link_to_parent")
        };

        (temp, root, src_dir, symlink_outside)
    }

    // ============================================================================
    // FsContext::new tests
    // ============================================================================

    #[test]
    fn test_fs_context_new_with_invalid_root() {
        let ctx = FsContext::new(
            PathBuf::from("/nonexistent/path/that/cannot/be/canonicalized"),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        // With invalid root, all access should be denied (empty whitelists)
        assert!(!ctx.can_read(&PathBuf::from("/tmp/somefile")));
        assert!(!ctx.can_write(&PathBuf::from("/tmp/somefile")));
    }

    // ============================================================================
    // Security: Path escaping with ..
    // ============================================================================

    #[test]
    fn test_resolve_escape_via_dots() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Try to escape using .. from workspace - ".." from "src" goes to root, which is allowed
        // So we need more ".." to actually escape
        let result = ctx.resolve_path_with_workspace(Path::new("../.."), Path::new("src"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside allowed root"));
    }

    #[test]
    fn test_resolve_escape_via_dots_in_path() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Try to escape using ../../.. in the path itself - this should escape
        let result = ctx.resolve_path_with_workspace(Path::new("../../.."), Path::new("src"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside allowed root"));
    }

    #[test]
    fn test_resolve_escape_via_dots_from_root() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Try to escape when workspace is empty (defaults to allowed_root)
        let result = ctx.resolve_path_with_workspace(Path::new(".."), Path::new(""));
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_nested_dots_escape() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Try deeply nested ..
        let result = ctx.resolve_path_with_workspace(Path::new("a/b/../../.."), Path::new("src"));
        assert!(result.is_err());
    }

    // ============================================================================
    // Security: Absolute paths outside allowed_root
    // ============================================================================

    #[test]
    fn test_absolute_path_outside_allowed_root() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Absolute path completely outside allowed_root
        let result = ctx.resolve_path_with_workspace(Path::new("/etc/passwd"), Path::new(""));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside allowed root"));
    }

    #[test]
    fn test_absolute_path_within_allowed_root() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Absolute path within allowed_root should work
        let secret_file = root.join("secret.txt");
        let result = ctx.resolve_path_with_workspace(&secret_file, Path::new(""));
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("secret.txt"));
    }

    // ============================================================================
    // Security: Symlink escaping
    // ============================================================================

    #[test]
    fn test_symlink_workspace_escape_blocked() {
        let (_temp, root, _src, symlink_outside) = create_test_dirs();

        // Verify symlink was created and points to parent
        #[cfg(unix)]
        {
            let target = std::fs::read_link(&symlink_outside).unwrap();
            assert_eq!(target, root, "symlink should point to root");
        }

        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Try to use workspace via symlink - workspace resolves to root,
        // then ".." from root would escape
        let result = ctx.resolve_path_with_workspace(Path::new(".."), Path::new("link_to_parent"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("outside allowed root"),
            "Error should mention 'outside allowed root', got: {}",
            err
        );
    }

    #[test]
    fn test_symlink_within_allowed_root_works() {
        let (_temp, root, _src, _symlink) = create_test_dirs();

        // Create a symlink that points WITHIN the allowed_root
        let link_inside = root.join("link_inside");
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("src"), &link_inside).unwrap();

        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Accessing a file through a symlink that's within allowed_root should work
        let result =
            ctx.resolve_path_with_workspace(Path::new("file.rs"), Path::new("link_inside"));
        assert!(
            result.is_ok(),
            "Accessing through symlink within allowed_root should work"
        );
    }

    // ============================================================================
    // Security: Workspace restrictions
    // ============================================================================

    #[test]
    fn test_workspace_outside_allowed_root_rejected() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Absolute workspace path outside allowed_root
        let result = ctx.resolve_path_with_workspace(Path::new("file.txt"), Path::new("/tmp"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside allowed root"));
    }

    #[test]
    fn test_empty_workspace_defaults_to_allowed_root() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Empty workspace should resolve relative to allowed_root
        let result = ctx.resolve_path_with_workspace(Path::new("secret.txt"), Path::new(""));
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("secret.txt"));
    }

    #[test]
    fn test_valid_relative_workspace() {
        let (_temp, root, _src_dir, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Valid workspace within allowed_root
        let result = ctx.resolve_path_with_workspace(Path::new("file.rs"), Path::new("src"));
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("file.rs"));
    }

    // ============================================================================
    // Permission: can_read / can_write
    // ============================================================================

    #[test]
    fn test_can_read_outside_allowed_root() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Path outside allowed_root should be denied
        assert!(!ctx.can_read(Path::new("/etc/passwd")));
    }

    #[test]
    fn test_can_write_outside_allowed_root() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Path outside allowed_root should be denied
        assert!(!ctx.can_write(Path::new("/etc/passwd")));
    }

    #[test]
    fn test_can_read_within_allowed_root_no_whitelist() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(
            root.clone(),
            Vec::new(), // empty read_whitelist
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        // Within allowed_root with empty whitelist should be allowed
        let secret_file = root.join("secret.txt");
        assert!(ctx.can_read(&secret_file));
    }

    #[test]
    fn test_can_write_within_allowed_root_no_whitelist() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(
            root.clone(),
            Vec::new(),
            Vec::new(), // empty write_whitelist
            Vec::new(),
            Vec::new(),
        );

        // Within allowed_root with empty whitelist should be allowed
        let new_file = root.join("new.txt");
        assert!(ctx.can_write(&new_file));
    }

    // ============================================================================
    // Permission: Whitelist restrictions
    // ============================================================================

    #[test]
    fn test_read_whitelist_allows_specific_path() {
        let (_temp, root, src_dir, _symlink) = create_test_dirs();
        let ctx = FsContext::new(
            root.clone(),
            vec![src_dir.clone()], // only src_dir is whitelisted for read
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        // File in whitelisted directory
        assert!(ctx.can_read(&src_dir.join("file.rs")));

        // File not in whitelisted directory
        assert!(!ctx.can_read(&root.join("secret.txt")));
    }

    #[test]
    fn test_write_whitelist_allows_specific_path() {
        let (_temp, root, src_dir, _symlink) = create_test_dirs();
        let ctx = FsContext::new(
            root.clone(),
            Vec::new(),
            vec![src_dir.clone()], // only src_dir is whitelisted for write
            Vec::new(),
            Vec::new(),
        );

        // File in whitelisted directory
        assert!(ctx.can_write(&src_dir.join("new.rs")));

        // File not in whitelisted directory
        assert!(!ctx.can_write(&root.join("secret.txt")));
    }

    #[test]
    fn test_whitelist_subdirectory_allowed() {
        let (_temp, root, src_dir, _symlink) = create_test_dirs();
        let ctx = FsContext::new(
            root.clone(),
            vec![root.clone()], // root is whitelisted, should include all subdirs
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        // Subdirectory should be accessible
        assert!(ctx.can_read(&src_dir.join("file.rs")));
    }

    // ============================================================================
    // Permission: Blacklist restrictions
    // ============================================================================

    #[test]
    fn test_read_blacklist_blocks_specific_path() {
        let (_temp, root, src_dir, _symlink) = create_test_dirs();
        let ctx = FsContext::new(
            root.clone(),
            Vec::new(), // empty whitelist means allow all within root
            Vec::new(),
            vec![root.join("secret.txt")], // blacklist secret.txt
            Vec::new(),
        );

        // Blacklisted file should be blocked
        assert!(!ctx.can_read(&root.join("secret.txt")));

        // Other files should be allowed
        assert!(ctx.can_read(&src_dir.join("file.rs")));
    }

    #[test]
    fn test_write_blacklist_blocks_specific_path() {
        let (_temp, root, src_dir, _symlink) = create_test_dirs();
        let ctx = FsContext::new(
            root.clone(),
            Vec::new(),
            Vec::new(), // empty whitelist means allow all within root
            Vec::new(),
            vec![root.join("secret.txt")], // blacklist secret.txt
        );

        // Blacklisted file should be blocked for writing
        assert!(!ctx.can_write(&root.join("secret.txt")));

        // Other files should be allowed
        assert!(ctx.can_write(&src_dir.join("file.rs")));
    }

    #[test]
    fn test_blacklist_takes_precedence_over_whitelist() {
        let (_temp, root, _src_dir, _symlink) = create_test_dirs();
        let ctx = FsContext::new(
            root.clone(),
            vec![root.join("secret.txt")], // whitelist says allow
            vec![root.join("secret.txt")], // whitelist says allow
            vec![root.join("secret.txt")], // but blacklist says deny
            vec![root.join("secret.txt")],
        );

        // Blacklist should take precedence
        assert!(!ctx.can_read(&root.join("secret.txt")));
        assert!(!ctx.can_write(&root.join("secret.txt")));
    }

    #[test]
    fn test_blacklist_directory_blocks_all_within() {
        let (_temp, root, src_dir, _symlink) = create_test_dirs();
        let ctx = FsContext::new(
            root.clone(),
            Vec::new(),
            Vec::new(),
            vec![src_dir.clone()], // blacklist entire src directory
            Vec::new(),
        );

        // Everything in blacklisted directory should be blocked
        assert!(!ctx.can_read(&src_dir.join("file.rs")));

        // Files outside should still work
        assert!(ctx.can_read(&root.join("secret.txt")));
    }

    // ============================================================================
    // Permission: Combined whitelist and blacklist
    // ============================================================================

    #[test]
    fn test_whitelist_allows_subdir_but_blacklist_blocks_deep() {
        let (_temp, root, _src, _symlink) = create_test_dirs();

        // Create a deeper structure
        let deep_dir = root.join("allowed").join("secret");
        fs::create_dir_all(&deep_dir).unwrap();
        fs::write(deep_dir.join("file.txt"), "secret").unwrap();

        let ctx = FsContext::new(
            root.clone(),
            vec![root.join("allowed")], // whitelist allows /allowed
            Vec::new(),
            vec![deep_dir.clone()], // blacklist blocks /allowed/secret
            Vec::new(),
        );

        // Should be blocked by blacklist even though parent is whitelisted
        assert!(!ctx.can_read(&deep_dir.join("file.txt")));
        assert!(ctx.can_read(&root.join("allowed").join("visible.txt")));
    }

    // ============================================================================
    // Edge cases
    // ============================================================================

    #[test]
    fn test_path_with_dot_special_case() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Single dot should work (current directory)
        let result = ctx.resolve_path_with_workspace(Path::new("."), Path::new("src"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_exact_allowed_root_boundary() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // The allowed_root itself should be accessible
        assert!(ctx.can_read(&root));
        assert!(ctx.can_write(&root));
    }

    #[test]
    fn test_multiple_dots_in_path() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Path with multiple ./
        let result = ctx.resolve_path_with_workspace(Path::new("./././secret.txt"), Path::new(""));
        assert!(result.is_ok());
    }

    #[test]
    fn test_path_with_trailing_slash() {
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let ctx = FsContext::new(root.clone(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

        // Path with trailing slash
        let result = ctx.resolve_path_with_workspace(Path::new("secret.txt/"), Path::new(""));
        assert!(result.is_ok());
    }

    // ============================================================================
    // FsContext Send + Sync safety
    // ============================================================================

    #[test]
    fn test_fs_context_is_send() {
        fn assert_send<T: Send>() {}
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let _ctx = FsContext::new(root, Vec::new(), Vec::new(), Vec::new(), Vec::new());
        assert_send::<FsContext>();
    }

    #[test]
    fn test_fs_context_is_sync() {
        fn assert_sync<T: Sync>() {}
        let (_temp, root, _src, _symlink) = create_test_dirs();
        let _ctx = FsContext::new(root, Vec::new(), Vec::new(), Vec::new(), Vec::new());
        assert_sync::<FsContext>();
    }
}

#[cfg(test)]
mod fs_integration_tests {
    use super::*;
    use extism::{Manifest, Plugin, Wasm};
    use serde::Deserialize;
    use std::fs;
    use tempfile::TempDir;

    // WASM file for test-fs plugin
    const TEST_FS_WASM: &[u8] =
        include_bytes!("../../../target/wasm32-unknown-unknown/release/test_fs.wasm");

    fn create_test_context(root: &std::path::Path) -> FsContext {
        FsContext::new(root.to_path_buf(), vec![], vec![], vec![], vec![])
    }

    fn run_plugin_with_fs<F>(root: &std::path::Path, f: F)
    where
        F: FnOnce(&mut Plugin),
    {
        let ctx = create_test_context(root);
        let functions = fs_functions(ctx);

        let manifest = Manifest::new([Wasm::data(TEST_FS_WASM)]);
        let mut plugin = Plugin::new(manifest, functions, true).unwrap();
        f(&mut plugin);
    }

    #[derive(Debug, Deserialize)]
    struct TestResult {
        success: bool,
        message: String,
    }

    #[test]
    fn test_integration_fs_write_read() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();

        run_plugin_with_fs(&root, |plugin: &mut Plugin| {
            let result: String = plugin.call("test_fs_write_read", "").unwrap();
            let result: TestResult = serde_json::from_str(&result).unwrap();
            assert!(result.success, "fs_write_read failed: {}", result.message);
        });
    }

    #[test]
    fn test_integration_fs_edit() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();

        run_plugin_with_fs(&root, |plugin: &mut Plugin| {
            let result: String = plugin.call("test_fs_edit", "").unwrap();
            let result: TestResult = serde_json::from_str(&result).unwrap();
            assert!(result.success, "fs_edit failed: {}", result.message);
            assert!(
                result.message.contains("Successfully"),
                "Edit did not succeed: {}",
                result.message
            );
        });
    }

    #[test]
    fn test_integration_fs_edit_all() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();

        run_plugin_with_fs(&root, |plugin: &mut Plugin| {
            let result: String = plugin.call("test_fs_edit_all", "").unwrap();
            let result: TestResult = serde_json::from_str(&result).unwrap();
            assert!(result.success, "fs_edit_all failed: {}", result.message);
            assert!(
                result.message.contains("replace_all ok"),
                "replace_all did not work: {}",
                result.message
            );
        });
    }

    #[test]
    fn test_integration_fs_list() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();

        // Create some files in the temp directory
        fs::write(root.join("test1.txt"), "content1").unwrap();
        fs::write(root.join("test2.txt"), "content2").unwrap();
        fs::create_dir_all(root.join("subdir")).unwrap();
        fs::write(root.join("subdir/nested.txt"), "nested").unwrap();

        run_plugin_with_fs(&root, |plugin: &mut Plugin| {
            let result: String = plugin.call("test_fs_list", "").unwrap();
            let result: TestResult = serde_json::from_str(&result).unwrap();
            assert!(result.success, "fs_list failed: {}", result.message);
        });
    }

    #[test]
    fn test_integration_fs_list_recursive() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();

        // Create a nested structure
        fs::create_dir_all(root.join("a/b/c")).unwrap();
        fs::write(root.join("a/file.txt"), "content").unwrap();
        fs::write(root.join("a/b/file.txt"), "content").unwrap();
        fs::write(root.join("a/b/c/file.txt"), "content").unwrap();

        run_plugin_with_fs(&root, |plugin: &mut Plugin| {
            let result: String = plugin.call("test_fs_list_recursive", "").unwrap();
            let result: TestResult = serde_json::from_str(&result).unwrap();
            assert!(
                result.success,
                "fs_list_recursive failed: {}",
                result.message
            );
            assert!(
                result.message.contains("entries shown:"),
                "Expected entry count: {}",
                result.message
            );
        });
    }

    #[test]
    fn test_integration_fs_read_pagination() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();

        run_plugin_with_fs(&root, |plugin: &mut Plugin| {
            let result: String = plugin.call("test_fs_read_pagination", "").unwrap();
            let result: TestResult = serde_json::from_str(&result).unwrap();
            assert!(
                result.success,
                "fs_read_pagination failed: {}",
                result.message
            );
            assert!(
                result.message.contains("pagination ok"),
                "Pagination did not work: {}",
                result.message
            );
        });
    }
}
