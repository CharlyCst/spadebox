use std::path::Path;

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use crate::{ToolResult, ToolError};

pub struct Sandbox {
    pub(crate) root: Dir,
}

impl Sandbox {
    /// Opens `path` as the jail root. All subsequent tool operations are
    /// confined to this directory — no ambient filesystem access occurs.
    pub fn new(path: impl AsRef<Path>) -> ToolResult<Self> {
        let root = Dir::open_ambient_dir(&path, ambient_authority())
            .map_err(|e| map_io_err(&path.as_ref().to_string_lossy(), e))?;
        Ok(Sandbox { root })
    }
}

/// Maps a raw `io::Error` from cap-std into a structured `ToolError`.
///
/// On Linux 5.6+, `cap-std` uses `openat2` with `RESOLVE_BENEATH`. The kernel
/// returns `EXDEV` (errno 18) when any path component (including symlinks)
/// attempts to escape the jail root. On older kernels and macOS, cap-std's
/// userspace resolver returns `EACCES` / `PermissionDenied` for escapes.
pub(crate) fn map_io_err(path: &str, e: std::io::Error) -> ToolError {
    const EXDEV: i32 = 18;
    if e.raw_os_error() == Some(EXDEV) {
        return ToolError::EscapeAttempt(path.to_string());
    }
    match e.kind() {
        std::io::ErrorKind::NotFound => ToolError::NotFound(path.to_string()),
        std::io::ErrorKind::PermissionDenied => ToolError::PermissionDenied(path.to_string()),
        _ => ToolError::IoError(e),
    }
}
