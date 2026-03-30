use std::path::Path;

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{Result, SpadeboxError};

pub struct Sandbox {
    root: Dir,
}

impl Sandbox {
    /// Opens `path` as the jail root. All subsequent tool operations are
    /// confined to this directory — no ambient filesystem access occurs.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let root = Dir::open_ambient_dir(path, ambient_authority()).map_err(map_io_err)?;
        Ok(Sandbox { root })
    }

    /// Reads the entire contents of `path` relative to the jail root.
    pub async fn read(&self, path: impl AsRef<Path>) -> Result<Vec<u8>> {
        // Open via cap-std (synchronous, capability-checked). The fd is
        // obtained before any await point, so `self.root` does not cross one.
        let file = self.root.open(path).map_err(map_io_err)?;
        let std_file = file.into_std();
        let mut tokio_file = tokio::fs::File::from_std(std_file);

        let mut buf = Vec::new();
        tokio_file
            .read_to_end(&mut buf)
            .await
            .map_err(SpadeboxError::IoError)?;
        Ok(buf)
    }

    /// Creates or truncates `path` relative to the jail root and writes `data`.
    pub async fn write(&self, path: impl AsRef<Path>, data: &[u8]) -> Result<()> {
        let file = self.root.create(path).map_err(map_io_err)?;
        let std_file = file.into_std();
        let mut tokio_file = tokio::fs::File::from_std(std_file);

        tokio_file
            .write_all(data)
            .await
            .map_err(SpadeboxError::IoError)?;
        Ok(())
    }
}

/// Maps a raw `io::Error` from cap-std into a structured `SpadeboxError`.
///
/// On Linux 5.6+, `cap-std` uses `openat2` with `RESOLVE_BENEATH`. The kernel
/// returns `EXDEV` (errno 18) when any path component (including symlinks)
/// attempts to escape the jail root. On older kernels and macOS, cap-std's
/// userspace resolver returns `EACCES` / `PermissionDenied` for escapes.
fn map_io_err(e: std::io::Error) -> SpadeboxError {
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
