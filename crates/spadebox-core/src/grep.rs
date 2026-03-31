//! Grep tool — sandboxed regex search across files.
//!
//! # Sandbox safety
//!
//! All filesystem access in this module goes through [`cap_std::fs::Dir`].
//! `Dir` methods (`read_dir`, `open_dir`, `open`) internally use `openat2` with
//! the `RESOLVE_BENEATH` flag on Linux 5.6+, which causes the kernel to reject
//! any path component that would escape the sandbox root — including symlinks,
//! `..` components, and absolute paths. On older kernels and macOS, cap-std's
//! userspace resolver enforces the same invariant.
//!
//! This module deliberately avoids `std::fs`, PathBuf-based opens, and the
//! `ignore` crate's directory walker (which resolves paths through ambient
//! `std::fs` internally, bypassing cap-std). The only path strings here are
//! relative strings accumulated for *display purposes only* — they are never
//! passed to any function that opens a file descriptor.
//!
//! The glob pattern and regex pattern are matched against plain strings /
//! compiled byte slices respectively. Neither can influence which file
//! descriptors are opened; all fd resolution is handled by cap-std.

use std::io;

use cap_std::fs::Dir;
use globset::{Glob, GlobSet, GlobSetBuilder};
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, SearcherBuilder, Sink, SinkContext, SinkContextKind, SinkMatch};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{sandbox::map_io_err, tools::Tool, Result, Sandbox, SpadeboxError};

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
        Search file contents for a regex pattern. \
        Returns matching lines with their file path and line number. \
        Use 'glob' to restrict the search to specific file types (e.g. '**/*.rs'). \
        Use 'context_lines' to include N surrounding lines around each match.";

    async fn run(sandbox: &Sandbox, params: GrepParams) -> Result<String> {
        // Compile the regex eagerly on the calling thread so we can return a
        // structured error before touching the filesystem.
        let matcher = RegexMatcher::new(&params.pattern)
            .map_err(|e| SpadeboxError::InvalidPattern(e.to_string()))?;

        // Build the glob set used for file-path filtering.
        // This is a pure string → DFA compilation step — no filesystem access.
        let glob_set = build_glob_set(params.glob.as_deref())?;

        // Clone the cap-std Dir so ownership can be moved into spawn_blocking.
        //
        // SANDBOX: `Dir::try_clone` duplicates the underlying file descriptor.
        // The cloned Dir carries the same `RESOLVE_BENEATH` constraint as the
        // original — all cap-std invariants are preserved across the clone.
        let root = sandbox
            .root
            .try_clone()
            .map_err(SpadeboxError::IoError)?;

        let context_lines = params.context_lines as usize;

        // Directory walking and grep-searcher are both synchronous. Run them on
        // a dedicated blocking thread to avoid stalling the async executor.
        let output = tokio::task::spawn_blocking(move || {
            let mut lines: Vec<String> = Vec::new();
            walk(&root, "", &matcher, &glob_set, context_lines, &mut lines)?;
            Ok::<String, SpadeboxError>(format_output(&lines))
        })
        .await
        .map_err(|e| SpadeboxError::IoError(io::Error::other(e)))??;

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Directory walker
// ---------------------------------------------------------------------------

/// Recursively walks `dir`, searching every file whose relative display path
/// matches `glob_set` for lines matching `matcher`.
///
/// # Sandbox safety
///
/// - `dir.read_dir(".")` lists entries of the *already-open* directory fd.
///   The `"."` argument is resolved relative to that fd by the kernel — no
///   ambient path lookup can reach outside `dir`.
/// - `dir.open_dir(name)` opens a subdirectory by entry name, fd-relative.
///   cap-std enforces `RESOLVE_BENEATH` so `.`, `..`, symlinks pointing
///   outside the jail, and absolute components are all rejected.
/// - `dir.open(name)` opens a file by entry name under the same constraints.
/// - `rel_path` / `child_rel` are display-only strings. They are assembled
///   from `DirEntry::file_name()` values and are **never** passed to any
///   function that resolves a path into a file descriptor.
fn walk(
    dir: &Dir,
    rel_path: &str,
    matcher: &RegexMatcher,
    glob_set: &GlobSet,
    context_lines: usize,
    out: &mut Vec<String>,
) -> Result<()> {
    // `read_dir(".")` enumerates entries of the already-open `dir` fd.
    // SANDBOX: resolved fd-relative; no ambient filesystem lookup.
    let entries = dir
        .read_dir(".")
        .map_err(|e| map_io_err(rel_path, e))?;

    for entry in entries {
        let entry = entry.map_err(SpadeboxError::IoError)?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Build the display path (forward-slash separated, relative to root).
        // Used only for output formatting and glob matching — never for opens.
        let child_rel = if rel_path.is_empty() {
            name_str.to_string()
        } else {
            format!("{}/{}", rel_path, name_str)
        };

        let file_type = entry.file_type().map_err(SpadeboxError::IoError)?;

        if file_type.is_dir() {
            // SANDBOX: `open_dir` is fd-relative and enforces `RESOLVE_BENEATH`.
            // A symlink pointing outside the sandbox root or a `..` component
            // will be rejected by the kernel before we ever recurse.
            let sub_dir = dir
                .open_dir(name_str.as_ref())
                .map_err(|e| map_io_err(&child_rel, e))?;
            walk(&sub_dir, &child_rel, matcher, glob_set, context_lines, out)?;
        } else if file_type.is_file() {
            // Apply the glob filter — pure string match, no filesystem access.
            if glob_set.is_match(&child_rel) {
                search_file(dir, name_str.as_ref(), &child_rel, matcher, context_lines, out)?;
            }
        }
        // Symlinks that resolve to neither file nor dir (broken links),
        // sockets, and device nodes are silently skipped.
    }

    Ok(())
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
) -> Result<()> {
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
        .map_err(SpadeboxError::IoError)
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
        if let Some(last) = self.last_line {
            if line_no > last + 1 {
                self.out.push("--".to_string());
            }
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
            .push(format!("{}:{}: {}", self.path, line_no, content.trim_end()));
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
            .push(format!("{}:{}- {}", self.path, line_no, content.trim_end()));
        self.last_line = Some(line_no);

        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compiles an optional glob pattern string into a [`GlobSet`].
///
/// When `glob` is `None`, returns a set that matches every file (`**/*`).
/// The set is matched against relative display paths (e.g. `"src/main.rs"`)
/// as plain string comparisons — no filesystem access is performed.
fn build_glob_set(glob: Option<&str>) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let pattern = glob.unwrap_or("**/*");
    builder.add(
        Glob::new(pattern).map_err(|e| SpadeboxError::InvalidPattern(e.to_string()))?,
    );
    builder
        .build()
        .map_err(|e| SpadeboxError::InvalidPattern(e.to_string()))
}

/// Formats collected match lines into the final output string.
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
        let sandbox = Sandbox::new(dir.path()).unwrap();
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

        assert!(result.contains("a.txt:1: hello world"), "got: {result}");
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

        assert!(result.contains("3: MATCH"), "got: {result}");
        assert!(result.contains("2- line two"), "got: {result}");
        assert!(result.contains("4- line four"), "got: {result}");
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

        assert!(matches!(result, Err(SpadeboxError::InvalidPattern(_))));
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
