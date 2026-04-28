use std::io::{self, Write};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Sandbox, ToolError, ToolResult, sandbox::map_io_err};

use crate::sandbox::Registry;
use crate::tool_utils::{self as fs_utils, deserialize_bool_flexible};

use super::Tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteParams {
    /// Path to the file to write, relative to the sandbox root.
    /// To create a directory instead of a file, end the path with '/' (e.g. 'src/').
    pub path: String,
    /// Content to write (UTF-8). Ignored when creating a directory.
    #[serde(default)]
    pub content: String,
    /// If true, create any missing intermediate directories before writing.
    /// Required when the parent directory does not yet exist.
    /// When the path ends with '/', creates the directory (and any parents) without writing a file.
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub create_dirs: bool,
}

pub struct WriteFileTool;

impl Tool for WriteFileTool {
    type Params = WriteParams;
    const NAME: &'static str = "write_file";
    const DESCRIPTION: &'static str = "Write text content to a file. \
        Provide a relative path (e.g. 'src/main.rs') and the full UTF-8 content to write. \
        Creates the file if it does not exist, or overwrites it entirely if it does. \
        If the file already exists, it must be read first — attempting to overwrite without a prior \
        read will return an error. \
        Set 'create_dirs' to true to create any missing intermediate directories automatically. \
        To create a directory without writing a file, end the path with '/' (e.g. 'src/utils/') \
        and set 'create_dirs' to true — content is ignored in that case.";

    async fn run(sandbox: &Sandbox, params: WriteParams) -> ToolResult<String> {
        // Clone the cap-std Dir so ownership can be moved into spawn_blocking.
        //
        // SANDBOX: `Dir::try_clone` duplicates the underlying file descriptor.
        // The cloned Dir carries the same `RESOLVE_BENEATH` constraint as the
        // original — all cap-std invariants are preserved across the clone.
        let root = sandbox.files.try_clone_root()?;
        let registry = Arc::clone(&sandbox.files.read_registry);

        // All filesystem operations (create_dir_all, create, write_all) are
        // blocking syscalls. Run them on a dedicated thread to avoid stalling
        // the async executor.
        tokio::task::spawn_blocking(move || do_write(root, params, &registry))
            .await
            .map_err(|e| ToolError::IoError(io::Error::other(e)))?
    }
}

/// Performs the actual filesystem work on a blocking thread.
///
/// Separated from `run` so the borrow checker is happy with the moved `root`
/// and `params`, and to keep the logic readable outside the async context.
fn do_write(
    root: cap_std::fs::Dir,
    mut params: WriteParams,
    registry: &Registry,
) -> ToolResult<String> {
    params.path = fs_utils::normalize_path(&params.path).to_string();

    if params.path.ends_with('/') {
        // Path ending with '/' is an explicit request to create a directory.
        // Trim the trailing slash for the cap-std call; create_dir_all handles
        // all intermediate components fd-relative under RESOLVE_BENEATH.
        let dir_path = params.path.trim_end_matches('/');
        root.create_dir_all(dir_path)
            .map_err(|e| map_io_err(dir_path, e))?;
        return Ok(format!("Created directory '{dir_path}'"));
    }

    if params.create_dirs {
        // Create any missing parent directories before opening the file.
        // std::path::Path::parent() is pure string manipulation here — no
        // filesystem call — so it is safe to use on the relative path string.
        if let Some(parent) = std::path::Path::new(&params.path).parent()
            && parent != std::path::Path::new("")
        {
            root.create_dir_all(parent)
                .map_err(|e| map_io_err(&parent.to_string_lossy(), e))?;
        }
    }

    // If the file already exists, enforce read-before-write and check for
    // external modifications by comparing the stored mtime against the current one.
    if let Ok(metadata) = root.metadata(&params.path) {
        let current_mtime = metadata.modified().map_err(ToolError::IoError)?;
        fs_utils::check_write_allowed(registry, &params.path, current_mtime)?;
    }

    let mut file = root
        .create(&params.path)
        .map_err(|e| map_io_err(&params.path, e))?;
    file.write_all(params.content.as_bytes())
        .map_err(ToolError::IoError)?;

    // Update the registry with the new mtime so subsequent writes are allowed.
    fs_utils::update_registry(registry, &params.path, &file)?;

    Ok(format!(
        "Wrote {} bytes to {}",
        params.content.len(),
        params.path
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sandbox;
    use crate::tools::read::{ReadFileTool, ReadParams};
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Sandbox) {
        let dir = TempDir::new().unwrap();
        let mut sandbox = Sandbox::new();
        sandbox.files.enable(dir.path()).unwrap();
        (dir, sandbox)
    }

    #[tokio::test]
    async fn writes_file() {
        let (dir, sandbox) = setup();
        WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "hello.txt".into(),
                content: "hello".into(),
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("hello.txt")).unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn creates_intermediate_dirs() {
        let (dir, sandbox) = setup();
        WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "a/b/c.txt".into(),
                content: "deep".into(),
                create_dirs: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("a/b/c.txt")).unwrap(),
            "deep"
        );
    }

    #[tokio::test]
    async fn leading_slash_is_stripped() {
        let (dir, sandbox) = setup();
        WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "/hello.txt".into(),
                content: "hello".into(),
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("hello.txt")).unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn fails_without_create_dirs_when_parent_missing() {
        let (_dir, sandbox) = setup();
        let result = WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "missing/file.txt".into(),
                content: "x".into(),
                create_dirs: false,
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn creates_directory_from_trailing_slash() {
        let (dir, sandbox) = setup();
        WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "new/nested/dir/".into(),
                content: String::new(),
                create_dirs: false, // trailing slash takes precedence
            },
        )
        .await
        .unwrap();
        assert!(dir.path().join("new/nested/dir").is_dir());
    }

    #[tokio::test]
    async fn content_defaults_to_empty() {
        let (dir, sandbox) = setup();
        let params: WriteParams = serde_json::from_str(r#"{"path":"empty.txt"}"#).unwrap();
        WriteFileTool::run(&sandbox, params).await.unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("empty.txt")).unwrap(),
            ""
        );
    }

    #[tokio::test]
    async fn errors_without_prior_read() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("existing.txt"), "content").unwrap();

        let result = WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "existing.txt".into(),
                content: "new content".into(),
                create_dirs: false,
            },
        )
        .await;

        assert!(matches!(result, Err(ToolError::NotRead(_))));
    }

    #[tokio::test]
    async fn errors_on_external_modification() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("f.txt"), "v1").unwrap();
        ReadFileTool::run(
            &sandbox,
            ReadParams {
                path: "f.txt".into(),
                limit: None,
                offset: None,
                max_bytes: None,
            },
        )
        .await
        .unwrap();

        // Simulate an external modification by bumping the mtime without
        // changing content, using std::fs::FileTimes (stable since Rust 1.75).
        let path = dir.path().join("f.txt");
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(
            fs::FileTimes::new().set_modified(mtime + std::time::Duration::from_secs(1)),
        )
        .unwrap();

        let result = WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "f.txt".into(),
                content: "v2".into(),
                create_dirs: false,
            },
        )
        .await;

        assert!(matches!(result, Err(ToolError::FileModified(_))));
    }

    #[tokio::test]
    async fn allows_consecutive_writes() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("f.txt"), "v1").unwrap();
        ReadFileTool::run(
            &sandbox,
            ReadParams {
                path: "f.txt".into(),
                limit: None,
                offset: None,
                max_bytes: None,
            },
        )
        .await
        .unwrap();

        WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "f.txt".into(),
                content: "v2".into(),
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "f.txt".into(),
                content: "v3".into(),
                create_dirs: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(fs::read_to_string(dir.path().join("f.txt")).unwrap(), "v3");
    }

    #[tokio::test]
    async fn allows_write_after_read() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("f.txt"), "v1").unwrap();
        ReadFileTool::run(
            &sandbox,
            ReadParams {
                path: "f.txt".into(),
                limit: None,
                offset: None,
                max_bytes: None,
            },
        )
        .await
        .unwrap();

        WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "f.txt".into(),
                content: "v2".into(),
                create_dirs: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(fs::read_to_string(dir.path().join("f.txt")).unwrap(), "v2");
    }
}
