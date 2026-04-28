//! Glob tool — sandboxed file discovery by pattern.
//!
//! This module also provides the shared [`walk`] and [`build_glob_set`]
//! primitives used by the [`super::grep`] tool. They live here because the
//! directory walker is the core of what the Glob tool does; Grep borrows it to
//! iterate over files before searching their content.
//!
//! # Sandbox safety
//!
//! All filesystem access goes through [`cap_std::fs::Dir`], whose methods
//! (`read_dir`, `open_dir`) use `openat2` with `RESOLVE_BENEATH` on Linux 5.6+,
//! rejecting any path component that would escape the sandbox root — including
//! symlinks, `..` components, and absolute paths. On older kernels and macOS,
//! cap-std's userspace resolver enforces the same invariant.
//!
//! The only strings in this module are relative display paths assembled from
//! `DirEntry::file_name()` values. They are used solely for glob matching and
//! output formatting — **never** passed to any function that opens a file
//! descriptor. All fd resolution is handled exclusively by cap-std.

use std::io;

use cap_std::fs::Dir;
use globset::{Glob, GlobSet, GlobSetBuilder};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Sandbox, ToolError, ToolResult, sandbox::map_io_err};

use super::Tool;

// ---------------------------------------------------------------------------
// Shared walker (also used by grep)
// ---------------------------------------------------------------------------

/// Recursively walks `dir`, calling `on_file(dir, entry_name, display_path)`
/// for each regular file whose relative display path matches `glob_set`.
///
/// - `dir`          — the already-open cap-std directory fd to walk.
/// - `rel_path` — display-only path of `dir` relative to the sandbox root
///   (empty string for the root itself).
/// - `glob_set` — compiled glob filter; only matching files trigger `on_file`.
/// - `on_file` — called with `(parent_dir, entry_name, display_path)`.
///   `parent_dir` and `entry_name` are provided so callers can
///   open the file through cap-std if needed (e.g. grep).
///   `display_path` is a forward-slash relative string for output.
///
/// # Sandbox safety
///
/// - `dir.read_dir(".")` enumerates the already-open fd — no ambient lookup.
/// - `dir.open_dir(name)` opens a subdirectory fd-relative under `RESOLVE_BENEATH`.
/// - `rel_path` / `child_rel` are display strings only, never used for opens.
/// - The `on_file` callback receives a cap-std `Dir` and a bare filename from
///   `DirEntry`, so any file open it performs is also fd-relative and
///   sandbox-enforced.
pub(super) fn walk<F>(
    dir: &Dir,
    rel_path: &str,
    glob_set: &GlobSet,
    on_file: &mut F,
) -> ToolResult<()>
where
    F: FnMut(&Dir, &str, &str) -> ToolResult<()>,
{
    // `read_dir(".")` enumerates entries of the already-open `dir` fd.
    // SANDBOX: resolved fd-relative; no ambient filesystem lookup.
    let entries = dir.read_dir(".").map_err(|e| map_io_err(rel_path, e))?;

    for entry in entries {
        let entry = entry.map_err(ToolError::IoError)?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Build the display path (forward-slash separated, relative to sandbox root).
        // Used only for glob matching and output formatting — never for opens.
        let child_rel = if rel_path.is_empty() {
            name_str.to_string()
        } else {
            format!("{}/{}", rel_path, name_str)
        };

        let file_type = entry.file_type().map_err(ToolError::IoError)?;

        if file_type.is_dir() {
            // SANDBOX: `open_dir` is fd-relative and enforces `RESOLVE_BENEATH`.
            // Symlinks pointing outside the sandbox root and `..` components are
            // rejected by the kernel before we recurse.
            let sub_dir = dir
                .open_dir(name_str.as_ref())
                .map_err(|e| map_io_err(&child_rel, e))?;
            walk(&sub_dir, &child_rel, glob_set, on_file)?;
        } else if file_type.is_file() {
            // Apply the glob filter — pure string match, no filesystem access.
            if glob_set.is_match(&child_rel) {
                on_file(dir, name_str.as_ref(), &child_rel)?;
            }
        }
        // Broken symlinks, sockets, and device nodes are silently skipped.
    }

    Ok(())
}

/// Compiles an optional glob pattern string into a [`GlobSet`].
///
/// When `glob` is `None`, returns a set that matches every file (`**/*`).
/// The set is matched against relative display paths (e.g. `"src/main.rs"`)
/// as pure string comparisons — no filesystem access is performed.
pub(super) fn build_glob_set(glob: Option<&str>) -> ToolResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let pattern = glob.unwrap_or("**/*");
    // Strip a leading slash: display paths are always relative (e.g. "src/main.rs"),
    // so a pattern like "/src/**" would never match without this normalization.
    let pattern = pattern.trim_start_matches('/');
    builder.add(Glob::new(pattern).map_err(|e| ToolError::InvalidPattern(e.to_string()))?);
    builder
        .build()
        .map_err(|e| ToolError::InvalidPattern(e.to_string()))
}

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GlobParams {
    /// Glob pattern to match file paths against
    /// (e.g. `"**/*.rs"`, `"src/**/*.ts"`, `"**/mod.rs"`).
    pub pattern: String,
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

pub struct GlobTool;

impl Tool for GlobTool {
    type Params = GlobParams;
    const NAME: &'static str = "glob";
    const DESCRIPTION: &'static str = "\
        List files matching a glob pattern. \
        Returns a newline-separated list of relative file paths sorted alphabetically. \
        Use '**' to match across directories (e.g. '**/*.rs' finds all Rust files, \
        'src/**/*.ts' finds TypeScript files under src/).";

    async fn run(sandbox: &Sandbox, params: GlobParams) -> ToolResult<String> {
        // Compile the glob pattern eagerly on the calling thread so we can
        // return a structured error before touching the filesystem.
        let glob_set = build_glob_set(Some(&params.pattern))?;

        // Clone the cap-std Dir so ownership can be moved into spawn_blocking.
        //
        // SANDBOX: `Dir::try_clone` duplicates the underlying file descriptor.
        // The cloned Dir carries the same `RESOLVE_BENEATH` constraint as the
        // original — all cap-std invariants are preserved across the clone.
        let root = sandbox.files.try_clone_root()?;

        // Directory walking is synchronous. Run on a dedicated blocking thread
        // to avoid stalling the async executor.
        let output = tokio::task::spawn_blocking(move || {
            let mut paths: Vec<String> = Vec::new();

            // The on_file callback only needs the display path — no file open required.
            walk(&root, "", &glob_set, &mut |_dir, _name, display_path| {
                paths.push(display_path.to_string());
                Ok(())
            })?;

            paths.sort();
            Ok::<String, ToolError>(format_output(&paths))
        })
        .await
        .map_err(|e| ToolError::IoError(io::Error::other(e)))??;

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_output(paths: &[String]) -> String {
    if paths.is_empty() {
        "No files found.".to_string()
    } else {
        paths.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
    async fn finds_matching_files() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("main.rs"), "").unwrap();
        fs::write(dir.path().join("lib.rs"), "").unwrap();
        fs::write(dir.path().join("readme.txt"), "").unwrap();

        let result = GlobTool::run(
            &sandbox,
            GlobParams {
                pattern: "**/*.rs".to_string(),
            },
        )
        .await
        .unwrap();

        assert!(result.contains("main.rs"), "got: {result}");
        assert!(result.contains("lib.rs"), "got: {result}");
        assert!(!result.contains("readme.txt"), "got: {result}");
    }

    #[tokio::test]
    async fn walks_subdirectories() {
        let (dir, sandbox) = setup();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/foo.rs"), "").unwrap();
        fs::write(dir.path().join("src/bar.rs"), "").unwrap();

        let result = GlobTool::run(
            &sandbox,
            GlobParams {
                pattern: "src/**/*.rs".to_string(),
            },
        )
        .await
        .unwrap();

        assert!(result.contains("src/foo.rs"), "got: {result}");
        assert!(result.contains("src/bar.rs"), "got: {result}");
    }

    #[tokio::test]
    async fn results_are_sorted() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("z.rs"), "").unwrap();
        fs::write(dir.path().join("a.rs"), "").unwrap();
        fs::write(dir.path().join("m.rs"), "").unwrap();

        let result = GlobTool::run(
            &sandbox,
            GlobParams {
                pattern: "**/*.rs".to_string(),
            },
        )
        .await
        .unwrap();

        let paths: Vec<&str> = result.lines().collect();
        assert_eq!(paths, vec!["a.rs", "m.rs", "z.rs"], "got: {result}");
    }

    #[tokio::test]
    async fn no_matches_returns_message() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("file.txt"), "").unwrap();

        let result = GlobTool::run(
            &sandbox,
            GlobParams {
                pattern: "**/*.rs".to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(result, "No files found.");
    }

    #[tokio::test]
    async fn leading_slash_in_pattern_is_stripped() {
        let (dir, sandbox) = setup();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/foo.rs"), "").unwrap();

        let result = GlobTool::run(
            &sandbox,
            GlobParams {
                pattern: "/src/**/*.rs".to_string(),
            },
        )
        .await
        .unwrap();

        assert!(result.contains("src/foo.rs"), "got: {result}");
    }

    #[tokio::test]
    async fn invalid_pattern_returns_error() {
        let (_dir, sandbox) = setup();
        let result = GlobTool::run(
            &sandbox,
            GlobParams {
                pattern: "[invalid".to_string(),
            },
        )
        .await;

        assert!(matches!(result, Err(ToolError::InvalidPattern(_))));
    }
}
