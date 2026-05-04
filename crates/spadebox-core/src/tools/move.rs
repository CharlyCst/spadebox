use std::io;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::tool_utils::Registry;
use crate::tool_utils::{self as fs_utils, deserialize_bool_flexible};
use crate::{Sandbox, ToolError, ToolResult, sandbox::map_io_err};

use super::Tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveParams {
    /// Source path (file or directory).
    pub src: String,
    /// Destination path, relative to the sandbox root.
    /// Required unless `delete` is true.
    pub dst: Option<String>,
    /// If true, overwrite the destination if it already exists.
    /// When overwriting an existing file, the destination must have been read first.
    /// Defaults to false.
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub overwrite: bool,
    /// If true and `dst` is omitted, delete `src` (file or directory) instead of moving it.
    /// Required when `dst` is absent, to confirm the deletion is intentional.
    /// Defaults to false.
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub delete: bool,
    /// If true, create any missing intermediate directories for the destination path.
    /// Defaults to false.
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub create_dirs: bool,
}

pub struct MoveTool;

impl Tool for MoveTool {
    type Params = MoveParams;
    const NAME: &'static str = "move";
    const DESCRIPTION: &'static str = "Move or rename a file or directory, or delete it. \
        Provide 'src' (source) and 'dst' (destination) to move or rename. \
        If 'dst' already exists and 'overwrite' is false (default), the call fails — \
        set 'overwrite' to true to replace it. \
        Set 'create_dirs' to true to create any missing intermediate directories for the destination. \
        To delete instead of moving, omit 'dst' and set 'delete' to true.";

    async fn run(sandbox: &Sandbox, params: MoveParams) -> ToolResult<String> {
        let root = sandbox.files.try_clone_root()?;
        let registry = Arc::clone(&sandbox.files.read_registry);

        tokio::task::spawn_blocking(move || do_move(root, params, &registry))
            .await
            .map_err(|e| ToolError::IoError(io::Error::other(e)))?
    }
}

fn do_move(
    root: cap_std::fs::Dir,
    mut params: MoveParams,
    registry: &Registry,
) -> ToolResult<String> {
    params.src = fs_utils::normalize_path(&params.src).to_string();

    if let Some(dst_raw) = params.dst {
        let dst = fs_utils::normalize_path(&dst_raw).to_string();

        // Check if destination already exists.
        if root.metadata(&dst).is_ok() && !params.overwrite {
            return Err(ToolError::IoError(io::Error::other(format!(
                "destination '{dst}' already exists; set overwrite to true to replace it"
            ))));
        }

        if params.create_dirs {
            if let Some(parent) = std::path::Path::new(&dst).parent()
                && parent != std::path::Path::new("")
            {
                root.create_dir_all(parent)
                    .map_err(|e| map_io_err(&parent.to_string_lossy(), e))?;
            }
        }

        // cap_std::Dir::rename takes two Dir handles (from_dir, from, to_dir, to).
        // SANDBOX: both handles share the same RESOLVE_BENEATH root, so the
        // rename cannot escape the sandbox regardless of the path values.
        let dst_dir = root.try_clone().map_err(ToolError::IoError)?;
        root.rename(&params.src, &dst_dir, &dst)
            .map_err(|e| map_io_err(&params.src, e))?;

        fs_utils::move_registry_entry(registry, &params.src, &dst, &dst_dir)?;

        Ok(format!("Moved '{}' to '{dst}'", params.src))
    } else {
        // Delete mode: dst omitted, delete must be explicitly set.
        if !params.delete {
            return Err(ToolError::IoError(io::Error::other(
                "missing destination: provide 'dst' to move, or set 'delete' to true to delete",
            )));
        }

        let meta = root
            .metadata(&params.src)
            .map_err(|e| map_io_err(&params.src, e))?;

        if meta.is_dir() {
            root.remove_dir_all(&params.src)
                .map_err(|e| map_io_err(&params.src, e))?;
        } else {
            root.remove_file(&params.src)
                .map_err(|e| map_io_err(&params.src, e))?;
        }

        registry.lock().unwrap().remove(&params.src);

        Ok(format!("Deleted '{}'", params.src))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sandbox;
    use crate::tools::read::{ReadFileTool, ReadParams};
    use crate::tools::write::{WriteFileTool, WriteParams};
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Sandbox) {
        let dir = TempDir::new().unwrap();
        let mut sandbox = Sandbox::new();
        sandbox.files.enable(dir.path()).unwrap();
        (dir, sandbox)
    }

    async fn read(sandbox: &Sandbox, path: &str) {
        ReadFileTool::run(
            sandbox,
            ReadParams {
                path: path.into(),
                limit: None,
                offset: None,
                max_bytes: None,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn moves_file() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        MoveTool::run(
            &sandbox,
            MoveParams {
                src: "a.txt".into(),
                dst: Some("b.txt".into()),
                overwrite: false,
                delete: false,
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        assert!(!dir.path().join("a.txt").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("b.txt")).unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn leading_slash_stripped() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        MoveTool::run(
            &sandbox,
            MoveParams {
                src: "/a.txt".into(),
                dst: Some("/b.txt".into()),
                overwrite: false,
                delete: false,
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        assert!(dir.path().join("b.txt").exists());
    }

    #[tokio::test]
    async fn fails_when_dst_exists_and_no_overwrite() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();
        let result = MoveTool::run(
            &sandbox,
            MoveParams {
                src: "a.txt".into(),
                dst: Some("b.txt".into()),
                overwrite: false,
                delete: false,
                create_dirs: false,
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn overwrite_succeeds_without_prior_read() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();
        MoveTool::run(
            &sandbox,
            MoveParams {
                src: "a.txt".into(),
                dst: Some("b.txt".into()),
                overwrite: true,
                delete: false,
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("b.txt")).unwrap(), "a");
    }

    #[tokio::test]
    async fn write_to_dst_allowed_after_move_when_src_was_read() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        read(&sandbox, "a.txt").await;
        MoveTool::run(
            &sandbox,
            MoveParams {
                src: "a.txt".into(),
                dst: Some("b.txt".into()),
                overwrite: false,
                delete: false,
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        // Should be able to write to dst immediately (registry entry transferred).
        WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "b.txt".into(),
                content: "world".into(),
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("b.txt")).unwrap(),
            "world"
        );
    }

    #[tokio::test]
    async fn write_to_dst_requires_read_when_src_was_not_read() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        MoveTool::run(
            &sandbox,
            MoveParams {
                src: "a.txt".into(),
                dst: Some("b.txt".into()),
                overwrite: false,
                delete: false,
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        let result = WriteFileTool::run(
            &sandbox,
            WriteParams {
                path: "b.txt".into(),
                content: "world".into(),
                create_dirs: false,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::NotRead(_))));
    }

    #[tokio::test]
    async fn deletes_file() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        MoveTool::run(
            &sandbox,
            MoveParams {
                src: "a.txt".into(),
                dst: None,
                overwrite: false,
                delete: true,
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        assert!(!dir.path().join("a.txt").exists());
    }

    #[tokio::test]
    async fn deletes_directory() {
        let (dir, sandbox) = setup();
        fs::create_dir(dir.path().join("mydir")).unwrap();
        fs::write(dir.path().join("mydir/f.txt"), "x").unwrap();
        MoveTool::run(
            &sandbox,
            MoveParams {
                src: "mydir".into(),
                dst: None,
                overwrite: false,
                delete: true,
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        assert!(!dir.path().join("mydir").exists());
    }

    #[tokio::test]
    async fn delete_requires_delete_flag() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let result = MoveTool::run(
            &sandbox,
            MoveParams {
                src: "a.txt".into(),
                dst: None,
                overwrite: false,
                delete: false,
                create_dirs: false,
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_dirs_makes_missing_parents() {
        let (dir, sandbox) = setup();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        MoveTool::run(
            &sandbox,
            MoveParams {
                src: "a.txt".into(),
                dst: Some("nested/dir/b.txt".into()),
                overwrite: false,
                delete: false,
                create_dirs: true,
            },
        )
        .await
        .unwrap();
        assert!(!dir.path().join("a.txt").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("nested/dir/b.txt")).unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn fails_when_dst_parent_missing_and_no_create_dirs() {
        let (_dir, sandbox) = setup();
        // Write a source file using std::fs directly so it pre-exists
        fs::write(_dir.path().join("a.txt"), "hello").unwrap();
        let result = MoveTool::run(
            &sandbox,
            MoveParams {
                src: "a.txt".into(),
                dst: Some("missing/b.txt".into()),
                overwrite: false,
                delete: false,
                create_dirs: false,
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn moves_directory() {
        let (dir, sandbox) = setup();
        fs::create_dir(dir.path().join("src_dir")).unwrap();
        fs::write(dir.path().join("src_dir/f.txt"), "content").unwrap();
        MoveTool::run(
            &sandbox,
            MoveParams {
                src: "src_dir".into(),
                dst: Some("dst_dir".into()),
                overwrite: false,
                delete: false,
                create_dirs: false,
            },
        )
        .await
        .unwrap();
        assert!(!dir.path().join("src_dir").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("dst_dir/f.txt")).unwrap(),
            "content"
        );
    }
}
