//! Grep tool — sandboxed regex search across files.
//!
//! Directory traversal and glob filtering are provided by [`super::glob::walk`]
//! and [`super::glob::build_glob_set`]. This module is responsible only for
//! opening each matched file and searching its content.
//!
//! # Sandbox safety
//!
//! All filesystem access goes through [`cap_std::fs::Dir`]. The shared walker
//! in [`super::glob`] uses only fd-relative `read_dir` / `open_dir` calls
//! (enforced via `openat2` with `RESOLVE_BENEATH` on Linux 5.6+), so no path
//! component can escape the sandbox root. File opens in this module use
//! `dir.open(name)` where `dir` and `name` come directly from the walker's
//! callback — also fd-relative and sandbox-enforced.
//!
//! This module deliberately avoids `std::fs`, PathBuf-based opens, and the
//! `ignore` crate's directory walker (which uses ambient `std::fs` paths
//! internally, bypassing cap-std). The regex pattern operates on already-opened
//! file descriptors and cannot influence which files are opened.

use std::io;

use cap_std::fs::Dir;
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, SearcherBuilder, Sink, SinkContext, SinkContextKind, SinkMatch};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Sandbox, ToolError, ToolResult, sandbox::map_io_err};

use super::{Tool, glob::build_glob_set, glob::walk};

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GrepParams {
    /// Regex pattern to search for (e.g. `"fn main"`, `"TODO.*fixme"`).
    pub pattern: String,

    /// Optional glob to restrict which files are searched
    /// (e.g. `"**/*.rs"`, `"src/**/*.ts"`). Matches all files when omitted.
    pub glob: Option<String>,

    /// Number of context lines to show before and after each match.
    /// Defaults to 0 (matched lines only).
    #[serde(default)]
    pub context_lines: u32,
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

pub struct GrepTool;

impl Tool for GrepTool {
    type Params = GrepParams;
    const NAME: &'static str = "grep";
    const DESCRIPTION: &'static str = "\
        Search file contents for a regex pattern (ripgrep). \
        Returns matching lines with their file path and line number. \
        Use 'glob' to restrict the search to specific file types (e.g. '**/*.rs'). \
        Use 'context_lines' to include N surrounding lines around each match.";

    async fn run(sandbox: &Sandbox, params: GrepParams) -> ToolResult<String> {
        // Compile the regex eagerly on the calling thread so we can return a
        // structured error before touching the filesystem.
        let matcher = RegexMatcher::new(&params.pattern)
            .map_err(|e| ToolError::InvalidPattern(e.to_string()))?;

        // Build the glob set used for file-path filtering.
        // This is a pure string → DFA compilation step — no filesystem access.
        let glob_set = build_glob_set(params.glob.as_deref())?;

        // Clone the cap-std Dir so ownership can be moved into spawn_blocking.
        //
        // SANDBOX: `Dir::try_clone` duplicates the underlying file descriptor.
        // The cloned Dir carries the same `RESOLVE_BENEATH` constraint as the
        // original — all cap-std invariants are preserved across the clone.
        let root = sandbox.files.try_clone_root()?;

        let context_lines = params.context_lines as usize;

        // Directory walking and grep-searcher are both synchronous. Run them on
        // a dedicated blocking thread to avoid stalling the async executor.
        let output = tokio::task::spawn_blocking(move || {
            let mut lines: Vec<String> = Vec::new();
            walk(&root, "", &glob_set, &mut |dir, name, display_path| {
                search_file(dir, name, display_path, &matcher, context_lines, &mut lines)
            })?;
            Ok::<String, ToolError>(format_output(&lines))
        })
        .await
        .map_err(|e| ToolError::IoError(io::Error::other(e)))??;

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// File searcher
// ---------------------------------------------------------------------------

/// Searches a single file for lines matching `matcher`, appending formatted
/// results to `out`.
///
/// # Sandbox safety
///
/// `dir.open(name)` opens `name` relative to the already-open `dir` fd.
/// cap-std enforces `RESOLVE_BENEATH`, so even if `name` somehow contained
/// adversarial components (it cannot here — it comes directly from a
/// `DirEntry` inside `dir`), the open would be rejected.
///
/// `grep-searcher` receives a `std::io::Read` derived from the cap-std `File`.
/// No path string is involved from this point onward — the search operates
/// entirely on the already-opened file descriptor.
fn search_file(
    dir: &Dir,
    name: &str,
    display_path: &str,
    matcher: &RegexMatcher,
    context_lines: usize,
    out: &mut Vec<String>,
) -> ToolResult<()> {
    // SANDBOX: fd-relative open, enforced by cap-std / RESOLVE_BENEATH.
    let file = dir.open(name).map_err(|e| map_io_err(display_path, e))?;

    // Convert to std::fs::File so grep-searcher can use it as `io::Read`.
    // The file descriptor is the same — only the wrapper type changes.
    // No path string is involved from this point onward.
    let std_file = file.into_std();

    let mut searcher = SearcherBuilder::new()
        .line_number(true)
        .before_context(context_lines)
        .after_context(context_lines)
        .build();

    let mut sink = MatchSink {
        path: display_path,
        out,
        last_line: None,
    };

    searcher
        .search_reader(matcher, std_file, &mut sink)
        .map_err(ToolError::IoError)
}

// ---------------------------------------------------------------------------
// Sink (match / context collector)
// ---------------------------------------------------------------------------

/// Collects matched and context lines into a flat `Vec<String>`.
///
/// Output format:
/// - Match lines:   `path/to/file.rs:42: content`  (`:` separator)
/// - Context lines: `path/to/file.rs:41- content`  (`-` separator, grep convention)
/// - Group gap:     `--`  (emitted between non-contiguous match groups)
struct MatchSink<'a> {
    path: &'a str,
    out: &'a mut Vec<String>,
    /// Line number of the most recently emitted line, used to detect gaps
    /// between match groups and insert `--` separators.
    last_line: Option<u64>,
}

impl MatchSink<'_> {
    /// Emits a `--` separator when `line_no` is not immediately contiguous
    /// with the last emitted line.
    fn maybe_separator(&mut self, line_no: u64) {
        if let Some(last) = self.last_line
            && line_no > last + 1
        {
            self.out.push("--".to_string());
        }
    }
}

impl Sink for MatchSink<'_> {
    type Error = io::Error;

    fn matched(
        &mut self,
        _searcher: &Searcher,
        mat: &SinkMatch<'_>,
    ) -> std::result::Result<bool, io::Error> {
        let line_no = mat.line_number().unwrap_or(0);
        self.maybe_separator(line_no);

        let content = String::from_utf8_lossy(mat.bytes());
        self.out
            .push(format!("{}:{}:{}", self.path, line_no, content.trim_end()));
        self.last_line = Some(line_no);

        Ok(true) // returning false would stop searching this file
    }

    fn context(
        &mut self,
        _searcher: &Searcher,
        ctx: &SinkContext<'_>,
    ) -> std::result::Result<bool, io::Error> {
        let line_no = ctx.line_number().unwrap_or(0);

        // Before-context lines precede a new match group. Check for a gap
        // against the previous group's after-context (or the previous match).
        if *ctx.kind() == SinkContextKind::Before {
            self.maybe_separator(line_no);
        }

        let content = String::from_utf8_lossy(ctx.bytes());
        self.out
            .push(format!("{}:{}-{}", self.path, line_no, content.trim_end()));
        self.last_line = Some(line_no);

        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_output(lines: &[String]) -> String {
    if lines.is_empty() {
        "No matches found.".to_string()
    } else {
        lines.join("\n")
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
    async fn finds_matching_lines() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("a.txt"), "hello world\ngoodbye world\n").unwrap();

        let result = GrepTool::run(
            &sandbox,
            GrepParams {
                pattern: "hello".to_string(),
                glob: None,
                context_lines: 0,
            },
        )
        .await
        .unwrap();

        assert!(result.contains("a.txt:1:hello world"), "got: {result}");
        assert!(!result.contains("goodbye"), "got: {result}");
    }

    #[tokio::test]
    async fn glob_filter_restricts_files() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("code.rs"), "let x = 1;\n").unwrap();
        fs::write(dir.path().join("note.txt"), "let x = 1;\n").unwrap();

        let result = GrepTool::run(
            &sandbox,
            GrepParams {
                pattern: "let x".to_string(),
                glob: Some("**/*.rs".to_string()),
                context_lines: 0,
            },
        )
        .await
        .unwrap();

        assert!(result.contains("code.rs"), "got: {result}");
        assert!(!result.contains("note.txt"), "got: {result}");
    }

    #[tokio::test]
    async fn no_matches_returns_message() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("empty.txt"), "nothing here\n").unwrap();

        let result = GrepTool::run(
            &sandbox,
            GrepParams {
                pattern: "xyzzy".to_string(),
                glob: None,
                context_lines: 0,
            },
        )
        .await
        .unwrap();

        assert_eq!(result, "No matches found.");
    }

    #[tokio::test]
    async fn context_lines_included() {
        let (dir, sandbox) = setup();
        fs::write(
            dir.path().join("ctx.txt"),
            "line one\nline two\nMATCH\nline four\nline five\n",
        )
        .unwrap();

        let result = GrepTool::run(
            &sandbox,
            GrepParams {
                pattern: "MATCH".to_string(),
                glob: None,
                context_lines: 1,
            },
        )
        .await
        .unwrap();

        assert!(result.contains("3:MATCH"), "got: {result}");
        assert!(result.contains("2-line two"), "got: {result}");
        assert!(result.contains("4-line four"), "got: {result}");
    }

    #[tokio::test]
    async fn invalid_regex_returns_error() {
        let (_dir, sandbox) = setup();
        let result = GrepTool::run(
            &sandbox,
            GrepParams {
                pattern: "[invalid".to_string(),
                glob: None,
                context_lines: 0,
            },
        )
        .await;

        assert!(matches!(result, Err(ToolError::InvalidPattern(_))));
    }

    #[tokio::test]
    async fn walks_subdirectories() {
        let (dir, sandbox) = setup();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/deep.rs"), "fn needle() {}\n").unwrap();

        let result = GrepTool::run(
            &sandbox,
            GrepParams {
                pattern: "needle".to_string(),
                glob: None,
                context_lines: 0,
            },
        )
        .await
        .unwrap();

        assert!(result.contains("sub/deep.rs:1:"), "got: {result}");
    }
}
