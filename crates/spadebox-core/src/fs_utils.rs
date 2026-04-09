use cap_std::fs::File;
use cap_std::time::SystemTime;

use crate::sandbox::Registry;
use crate::{ToolError, ToolResult};

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
