use std::io::{self, Read, Write};

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{sandbox::map_io_err, ToolResult, Sandbox, SpadeboxError};

use super::{deserialize_bool_flexible, Tool};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditParams {
    /// Path to the file to edit, relative to the sandbox root.
    pub path: String,
    /// Exact string to search for. Must appear exactly once unless replace_all is true.
    /// Include enough surrounding context to uniquely identify the location.
    pub old_string: String,
    /// String to replace it with.
    pub new_string: String,
    /// If true, replace every occurrence instead of requiring exactly one. Defaults to false.
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub replace_all: bool,
}

pub struct EditFileTool;

impl Tool for EditFileTool {
    type Params = EditParams;
    const NAME: &'static str = "edit_file";
    const DESCRIPTION: &'static str = "Replace text within a file. Reads the file, finds the exact string provided in 'old_string', \
         and replaces it with 'new_string'. By default the string must appear exactly once — include \
         enough surrounding context in 'old_string' to make it unique. \
         If the string appears multiple times and you want to replace all of them, set replace_all to true. \
         Always read the file before editing to ensure 'old_string' matches the current content exactly.";

    async fn run(sandbox: &Sandbox, params: EditParams) -> ToolResult<String> {
        // Clone the cap-std Dir so ownership can be moved into spawn_blocking.
        //
        // SANDBOX: `Dir::try_clone` duplicates the underlying file descriptor.
        // The cloned Dir carries the same `RESOLVE_BENEATH` constraint as the
        // original — all cap-std invariants are preserved across the clone.
        let root = sandbox.root.try_clone().map_err(SpadeboxError::IoError)?;

        // open(), read_to_end(), create(), and write_all() are all blocking
        // syscalls. Run them on a dedicated thread to avoid stalling the executor.
        tokio::task::spawn_blocking(move || do_edit(root, params))
            .await
            .map_err(|e| SpadeboxError::IoError(io::Error::other(e)))?
    }
}

fn do_edit(root: cap_std::fs::Dir, params: EditParams) -> ToolResult<String> {
    // SANDBOX: fd-relative open enforced by cap-std / RESOLVE_BENEATH.
    let mut file = root
        .open(&params.path)
        .map_err(|e| map_io_err(&params.path, e))?;

    // `cap_std::fs::File` implements `std::io::Read` by calling the `read`
    // syscall on the already-open file descriptor. No path resolution occurs —
    // the sandbox guarantee was established at `open()` time above.
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(SpadeboxError::IoError)?;
    let content =
        String::from_utf8(buf).map_err(|_| SpadeboxError::NotUtf8(params.path.clone()))?;

    // Validate — pure in-memory string operations, no filesystem access.
    let count = content.matches(params.old_string.as_str()).count();
    match count {
        0 => return Err(SpadeboxError::StringNotFound(params.path.clone())),
        n if n > 1 && !params.replace_all => {
            return Err(SpadeboxError::AmbiguousEdit {
                path: params.path.clone(),
                count: n,
            });
        }
        _ => {}
    }

    // Replace — pure in-memory, no filesystem access.
    let updated = if params.replace_all {
        content.replace(params.old_string.as_str(), &params.new_string)
    } else {
        content.replacen(params.old_string.as_str(), &params.new_string, 1)
    };

    // SANDBOX: fd-relative create enforced by cap-std / RESOLVE_BENEATH.
    let mut file = root
        .create(&params.path)
        .map_err(|e| map_io_err(&params.path, e))?;

    // `cap_std::fs::File` implements `std::io::Write` by calling the `write`
    // syscall on the already-open file descriptor. No path resolution occurs —
    // the sandbox guarantee was established at `create()` time above.
    file.write_all(updated.as_bytes())
        .map_err(SpadeboxError::IoError)?;

    let replacements = if params.replace_all { count } else { 1 };
    Ok(format!(
        "Replaced {} occurrence(s) in '{}'",
        replacements, params.path
    ))
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
    async fn replaces_string() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("f.txt"), "hello world").unwrap();

        EditFileTool::run(&sandbox, EditParams {
            path: "f.txt".into(),
            old_string: "world".into(),
            new_string: "rust".into(),
            replace_all: false,
        }).await.unwrap();

        assert_eq!(fs::read_to_string(dir.path().join("f.txt")).unwrap(), "hello rust");
    }

    #[tokio::test]
    async fn replace_all_replaces_every_occurrence() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("f.txt"), "a a a").unwrap();

        EditFileTool::run(&sandbox, EditParams {
            path: "f.txt".into(),
            old_string: "a".into(),
            new_string: "b".into(),
            replace_all: true,
        }).await.unwrap();

        assert_eq!(fs::read_to_string(dir.path().join("f.txt")).unwrap(), "b b b");
    }

    #[tokio::test]
    async fn errors_on_ambiguous_match() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("f.txt"), "a a").unwrap();

        let result = EditFileTool::run(&sandbox, EditParams {
            path: "f.txt".into(),
            old_string: "a".into(),
            new_string: "b".into(),
            replace_all: false,
        }).await;

        assert!(matches!(result, Err(SpadeboxError::AmbiguousEdit { .. })));
    }

    #[tokio::test]
    async fn errors_on_string_not_found() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("f.txt"), "hello").unwrap();

        let result = EditFileTool::run(&sandbox, EditParams {
            path: "f.txt".into(),
            old_string: "xyzzy".into(),
            new_string: "b".into(),
            replace_all: false,
        }).await;

        assert!(matches!(result, Err(SpadeboxError::StringNotFound(_))));
    }

    use super::EditParams;

    #[test]
    fn deserialize_bool_flexible() {
        fn parse(replace_all: &str) -> EditParams {
            serde_json::from_str(&format!(
                r#"{{"path":"f","old_string":"a","new_string":"b","replace_all":{replace_all}}}"#
            ))
            .unwrap()
        }

        // JSON booleans
        assert!(parse("true").replace_all);
        assert!(!parse("false").replace_all);

        // String booleans (sent by some MCP clients)
        assert!(parse(r#""true""#).replace_all);
        assert!(!parse(r#""false""#).replace_all);

        // Absent field defaults to false
        let p: EditParams = serde_json::from_str(r#"{"path":"f","old_string":"a","new_string":"b"}"#).unwrap();
        assert!(!p.replace_all);

        // Invalid string is rejected
        let result: serde_json::Result<EditParams> =
            serde_json::from_str(r#"{"path":"f","old_string":"a","new_string":"b","replace_all":"yes"}"#);
        assert!(result.is_err());
    }
}
