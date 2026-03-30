use std::path::Path;

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use crate::{Result, SpadeboxError};

pub struct Sandbox {
    pub(crate) root: Dir,
}

impl Sandbox {
    /// Opens `path` as the jail root. All subsequent tool operations are
    /// confined to this directory — no ambient filesystem access occurs.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let root = Dir::open_ambient_dir(path, ambient_authority()).map_err(map_io_err)?;
        Ok(Sandbox { root })
    }
}

/// Maps a raw `io::Error` from cap-std into a structured `SpadeboxError`.
///
/// On Linux 5.6+, `cap-std` uses `openat2` with `RESOLVE_BENEATH`. The kernel
/// returns `EXDEV` (errno 18) when any path component (including symlinks)
/// attempts to escape the jail root. On older kernels and macOS, cap-std's
/// userspace resolver returns `EACCES` / `PermissionDenied` for escapes.
pub(crate) fn map_io_err(e: std::io::Error) -> SpadeboxError {
    const EXDEV: i32 = 18;
    if e.raw_os_error() == Some(EXDEV) {
        return SpadeboxError::EscapeAttempt;
    }
    match e.kind() {
        std::io::ErrorKind::NotFound => SpadeboxError::NotFound,
        std::io::ErrorKind::PermissionDenied => SpadeboxError::PermissionDenied,
        _ => SpadeboxError::IoError(e),
    }
}
