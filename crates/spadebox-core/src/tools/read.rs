use std::io::{self, Read};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Sandbox, ToolError, ToolResult, sandbox::map_io_err};

use super::Tool;

/// Default byte cap applied to the output of every read. Large enough for
/// virtually any source file; small enough to protect the context window.
pub const DEFAULT_MAX_BYTES: u64 = 20_000;

/// String appended at the end of the read when file size exceeds the `max_bytes` parameter.
const TRUNCATION_WARNING: &str =
    "\n<warning>The file has been truncated due to max_bytes limit</warning>";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadParams {
    /// Path to the file to read, relative to the sandbox root.
    pub path: String,
    /// Maximum number of lines to return. Omit to read the entire file.
    pub limit: Option<u64>,
    /// 1-indexed line number to start reading from. Defaults to 1 (the beginning of the file).
    pub offset: Option<u64>,
    /// Maximum number of bytes to return (applied after `offset`/`limit` windowing).
    /// Defaults to 20 000. Set to 0 to disable.
    pub max_bytes: Option<u64>,
}

pub struct ReadFileTool;

impl Tool for ReadFileTool {
    type Params = ReadParams;
    const NAME: &'static str = "read_file";
    const DESCRIPTION: &'static str = "Read the text content of a file. \
         Provide a relative path (e.g. 'src/main.rs' or 'README.md'). \
         Returns the file's content as a UTF-8 string. \
         Use `offset` (1-indexed) and `limit` to read a specific range of lines.";

    async fn run(sandbox: &Sandbox, params: ReadParams) -> ToolResult<String> {
        // Clone the cap-std Dir so ownership can be moved into spawn_blocking.
        //
        // SANDBOX: `Dir::try_clone` duplicates the underlying file descriptor.
        // The cloned Dir carries the same `RESOLVE_BENEATH` constraint as the
        // original — all cap-std invariants are preserved across the clone.
        let root = sandbox.files.try_clone_root()?;
        let registry = Arc::clone(&sandbox.files.read_registry);

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
            // Always recorded against the full file regardless of offset/limit.
            let mtime = file
                .metadata()
                .and_then(|m| m.modified())
                .map_err(ToolError::IoError)?;
            registry.lock().unwrap().insert(params.path.clone(), mtime);

            let content = String::from_utf8_lossy(&buf).into_owned();
            let windowed = apply_window(content, params.offset, params.limit);
            Ok(truncate_bytes(
                windowed,
                params.max_bytes.unwrap_or(DEFAULT_MAX_BYTES),
            ))
        })
        .await
        .map_err(|e| ToolError::IoError(io::Error::other(e)))?
    }
}

/// Truncate `content` to at most `max_bytes` bytes, respecting UTF-8 character
/// boundaries. If truncation occurs, a warning marker is appended. Pass `0`
/// to disable the limit.
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

/// Apply an optional line window to `content`.
///
/// `offset` is 1-indexed (line 1 = first line of the file).
/// `limit` caps the number of lines returned.
pub(crate) fn apply_window(content: String, offset: Option<u64>, limit: Option<u64>) -> String {
    match (offset, limit) {
        (None, None) => content,
        (offset, limit) => {
            // offset is 1-indexed; convert to 0-indexed start.
            let start = offset.unwrap_or(1).saturating_sub(1) as usize;
            let lines: Vec<&str> = content.lines().collect();
            let slice = &lines[start.min(lines.len())..];
            let slice = match limit {
                Some(n) => &slice[..slice.len().min(n as usize)],
                None => slice,
            };
            slice.join("\n")
        }
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
        let mut sandbox = Sandbox::new();
        sandbox.files.enable(dir.path()).unwrap();
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
                limit: None,
                offset: None,
                max_bytes: None,
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
                limit: None,
                offset: None,
                max_bytes: None,
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
                limit: None,
                offset: None,
                max_bytes: None,
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
                limit: None,
                offset: None,
                max_bytes: None,
            },
        )
        .await;

        assert!(matches!(
            result,
            Err(ToolError::EscapeAttempt(_) | ToolError::PermissionDenied(_))
        ));
    }

    #[test]
    fn apply_window_cases() {
        let content = "line1\nline2\nline3\nline4\nline5".to_string();

        // limit only
        assert_eq!(apply_window(content.clone(), None, Some(2)), "line1\nline2");
        // offset only (1-indexed)
        assert_eq!(
            apply_window(content.clone(), Some(3), None),
            "line3\nline4\nline5"
        );
        // both
        assert_eq!(
            apply_window(content.clone(), Some(2), Some(2)),
            "line2\nline3"
        );
        // offset beyond end
        assert_eq!(apply_window(content.clone(), Some(100), None), "");
        // neither — passthrough
        assert_eq!(apply_window(content.clone(), None, None), content);
    }

    #[test]
    fn truncate_bytes_cases() {
        let warning = TRUNCATION_WARNING;

        // under limit — passthrough
        assert_eq!(truncate_bytes("hello".into(), 100), "hello");
        // exact limit — passthrough
        assert_eq!(truncate_bytes("hello".into(), 5), "hello");
        // over limit — truncate and warn
        assert_eq!(
            truncate_bytes("abcdefghij".into(), 5),
            format!("abcde{warning}")
        );
        // zero disables limit
        let long = "a".repeat(100_000);
        assert_eq!(truncate_bytes(long.clone(), 0), long);
        // respects UTF-8 boundary: "é" is 2 bytes, cutting at byte 2 must not split it
        assert_eq!(truncate_bytes("aé".into(), 2), format!("a{warning}"));
    }

    #[tokio::test]
    async fn max_bytes_cases() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("file.txt"), "abcdefghij").unwrap();

        let read = |max_bytes| {
            ReadFileTool::run(
                &sandbox,
                ReadParams {
                    path: "file.txt".into(),
                    limit: None,
                    offset: None,
                    max_bytes,
                },
            )
        };

        // truncates and warns
        assert_eq!(
            read(Some(5)).await.unwrap(),
            format!("abcde{TRUNCATION_WARNING}")
        );
        // zero disables limit
        assert_eq!(read(Some(0)).await.unwrap(), "abcdefghij");
        // None uses the default (file is well under 20 000 bytes)
        assert_eq!(read(None).await.unwrap(), "abcdefghij");
    }
}
