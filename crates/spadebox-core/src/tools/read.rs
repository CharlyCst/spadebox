use std::io::{self, Read};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Sandbox, ToolError, ToolResult, sandbox::map_io_err};

use super::Tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadParams {
    /// Path to the file to read, relative to the sandbox root.
    pub path: String,
}

pub struct ReadFileTool;

impl Tool for ReadFileTool {
    type Params = ReadParams;
    const NAME: &'static str = "read_file";
    const DESCRIPTION: &'static str = "Read the full text content of a file. \
         Provide a relative path (e.g. 'src/main.rs' or 'README.md'). \
         Returns the file's content as a UTF-8 string.";

    async fn run(sandbox: &Sandbox, params: ReadParams) -> ToolResult<String> {
        // Clone the cap-std Dir so ownership can be moved into spawn_blocking.
        //
        // SANDBOX: `Dir::try_clone` duplicates the underlying file descriptor.
        // The cloned Dir carries the same `RESOLVE_BENEATH` constraint as the
        // original — all cap-std invariants are preserved across the clone.
        let root = sandbox.root.try_clone().map_err(ToolError::IoError)?;
        let registry = Arc::clone(&sandbox.read_registry);

        // open() and read_to_end() are both blocking syscalls. Run them on a
        // dedicated thread to avoid stalling the async executor.
        tokio::task::spawn_blocking(move || {
            // SANDBOX: fd-relative open enforced by cap-std / RESOLVE_BENEATH.
            let mut file = root
                .open(&params.path)
                .map_err(|e| map_io_err(&params.path, e))?;

            // `cap_std::fs::File` implements `std::io::Read` by calling the
            // `read` syscall on the already-open file descriptor. No path
            // resolution occurs here — the sandbox guarantee was established
            // at `open()` time above.
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).map_err(ToolError::IoError)?;

            // Record the mtime so write/edit tools can detect external modifications.
            let mtime = file
                .metadata()
                .and_then(|m| m.modified())
                .map_err(ToolError::IoError)?;
            registry.lock().unwrap().insert(params.path.clone(), mtime);

            Ok(String::from_utf8_lossy(&buf).into_owned())
        })
        .await
        .map_err(|e| ToolError::IoError(io::Error::other(e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sandbox;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Sandbox) {
        let dir = TempDir::new().unwrap();
        let sandbox = Sandbox::new(dir.path()).unwrap();
        (dir, sandbox)
    }

    #[tokio::test]
    async fn reads_file_content() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("hello.txt"), "hello world").unwrap();

        let result = ReadFileTool::run(
            &sandbox,
            ReadParams {
                path: "hello.txt".into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(result, "hello world");
    }

    #[tokio::test]
    async fn reads_file_in_subdirectory() {
        let (dir, sandbox) = setup();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/file.txt"), "nested").unwrap();

        let result = ReadFileTool::run(
            &sandbox,
            ReadParams {
                path: "sub/file.txt".into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(result, "nested");
    }

    #[tokio::test]
    async fn errors_on_missing_file() {
        let (_dir, sandbox) = setup();

        let result = ReadFileTool::run(
            &sandbox,
            ReadParams {
                path: "nope.txt".into(),
            },
        )
        .await;

        assert!(matches!(result, Err(ToolError::NotFound(_))));
    }

    #[tokio::test]
    async fn rejects_path_traversal() {
        let (_dir, sandbox) = setup();

        let result = ReadFileTool::run(
            &sandbox,
            ReadParams {
                path: "../etc/passwd".into(),
            },
        )
        .await;

        assert!(matches!(
            result,
            Err(ToolError::EscapeAttempt(_) | ToolError::PermissionDenied(_))
        ));
    }
}
