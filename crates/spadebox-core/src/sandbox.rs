use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use cap_std::time::SystemTime;

use crate::{ToolError, ToolResult};

/// Registry mapping relative file paths to the mtime recorded at last read.
///
/// Used to enforce read-before-write and detect external modifications.
/// The inner `Mutex` is a `std::sync::Mutex` (not `tokio::sync::Mutex`) because it
/// is only ever locked on blocking threads inside `spawn_blocking`. Never lock it
/// across an `.await` point — that would block the async executor.
pub(crate) type Registry = Arc<Mutex<HashMap<String, SystemTime>>>;

pub struct Sandbox {
    pub(crate) root: Dir,
    pub(crate) read_registry: Registry,
}

impl Sandbox {
    /// Opens `path` as the jail root. All subsequent tool operations are
    /// confined to this directory — no ambient filesystem access occurs.
    pub fn new(path: impl AsRef<Path>) -> ToolResult<Self> {
        let root = Dir::open_ambient_dir(&path, ambient_authority())
            .map_err(|e| map_io_err(&path.as_ref().to_string_lossy(), e))?;
        Ok(Sandbox {
            root,
            read_registry: Arc::new(Mutex::new(HashMap::new())),
        })
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
