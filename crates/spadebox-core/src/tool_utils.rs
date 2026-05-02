use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Deserialize;

use cap_std::fs::{Dir, File};
use cap_std::time::SystemTime;

use crate::{ToolError, ToolResult};

/// Registry mapping relative file paths to the mtime recorded at last read.
///
/// Used to enforce read-before-write and detect external modifications.
/// The inner `Mutex` is a `std::sync::Mutex` (not `tokio::sync::Mutex`) because it
/// is only ever locked on blocking threads inside `spawn_blocking`. Never lock it
/// across an `.await` point — that would block the async executor.
pub(crate) type Registry = Arc<Mutex<HashMap<String, SystemTime>>>;

/// Default byte cap applied to tool outputs. Large enough for virtually any
/// source file; small enough to protect the context window.
pub const DEFAULT_MAX_BYTES: u64 = 20_000;

/// Strip a leading `/` from a path so that sandbox-root-relative paths like
/// `/src/main.rs` are treated the same as `src/main.rs`.
///
/// cap-std's `RESOLVE_BENEATH` enforcement rejects absolute paths, so without
/// this normalization any tool call that includes a leading slash would fail.
pub(crate) fn normalize_path(path: &str) -> &str {
    path.trim_start_matches('/')
}

/// String appended at the end of the output when it is truncated by `max_bytes`.
pub(crate) const TRUNCATION_WARNING: &str =
    "\n<warning>The file has been truncated due to max_bytes limit</warning>";

/// Truncate `content` to at most `max_bytes` bytes, respecting UTF-8 character
/// boundaries. If truncation occurs, [`TRUNCATION_WARNING`] is appended. Pass
/// `0` to disable the limit.
pub(crate) fn truncate_bytes(content: String, max_bytes: u64) -> String {
    let limit = max_bytes as usize;
    if limit == 0 || content.len() <= limit {
        return content;
    }
    // Walk back from the limit to the nearest UTF-8 character boundary.
    let mut end = limit;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = content[..end].to_string();
    truncated.push_str(TRUNCATION_WARNING);
    truncated
}

/// Deserializes a boolean that may arrive as a JSON bool or as a string.
/// MCP clients such as Claude Code may serialize booleans as strings (`"true"`).
pub(crate) fn deserialize_bool_flexible<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrString {
        Bool(bool),
        Str(String),
    }
    match BoolOrString::deserialize(d)? {
        BoolOrString::Bool(b) => Ok(b),
        BoolOrString::Str(s) => s.parse().map_err(serde::de::Error::custom),
    }
}

/// Checks that `path` was read before writing and has not been modified externally.
///
/// Compares the mtime stored in the registry against the file's current mtime.
/// Returns `NotRead` if the file was never read this session, or `FileModified`
/// if the file changed since the last read.
///
/// New files (not yet on disk) are exempt — only call this when the file already
/// exists (i.e. after a successful `root.metadata(path)`).
///
/// # Async safety
/// Must be called from a `spawn_blocking` closure, never directly in async code.
pub(crate) fn check_write_allowed(
    registry: &Registry,
    path: &str,
    current_mtime: SystemTime,
) -> ToolResult<()> {
    let recorded = registry.lock().unwrap().get(path).copied();
    match recorded {
        None => Err(ToolError::NotRead(path.to_string())),
        Some(recorded) if recorded != current_mtime => {
            Err(ToolError::FileModified(path.to_string()))
        }
        _ => Ok(()),
    }
}

/// Updates the registry with the mtime of `file` after a successful write.
///
/// Must be called with the file still open so the mtime is read from the
/// already-open file descriptor without a path lookup.
///
/// # Async safety
/// Must be called from a `spawn_blocking` closure, never directly in async code.
pub(crate) fn update_registry(registry: &Registry, path: &str, file: &File) -> ToolResult<()> {
    let mtime = file
        .metadata()
        .and_then(|m| m.modified())
        .map_err(ToolError::IoError)?;
    registry.lock().unwrap().insert(path.to_string(), mtime);
    Ok(())
}

/// Transfers the registry entry from `src` to `dst` after a successful rename.
///
/// If `src` had a registry entry, `dst` is registered with its current mtime
/// (read from `dst_dir`) so the caller can write to the destination immediately.
/// If `src` had no entry, any stale `dst` entry is removed — the content at
/// `dst` has changed and must be read before writing.
///
/// # Async safety
/// Must be called from a `spawn_blocking` closure, never directly in async code.
pub(crate) fn move_registry_entry(
    registry: &Registry,
    src: &str,
    dst: &str,
    dst_dir: &Dir,
) -> ToolResult<()> {
    let mut reg = registry.lock().unwrap();
    let had_src = reg.remove(src).is_some();
    if had_src {
        if let Ok(meta) = dst_dir.metadata(dst)
            && let Ok(mtime) = meta.modified()
        {
            reg.insert(dst.to_string(), mtime);
        }
    } else {
        reg.remove(dst);
    }
    Ok(())
}
