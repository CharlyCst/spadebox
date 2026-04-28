use cap_std::fs::File;
use cap_std::time::SystemTime;

use crate::sandbox::Registry;
use crate::{ToolError, ToolResult};

// ---------------------------------------------------------------------------
// Output truncation
// ---------------------------------------------------------------------------

/// Default byte cap applied to tool outputs. Large enough for virtually any
/// source file; small enough to protect the context window.
pub const DEFAULT_MAX_BYTES: u64 = 20_000;

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
