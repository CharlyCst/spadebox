use std::io::Write;
use std::sync::Arc;

use boa_engine::{
    Context, JsNativeError, JsResult, JsValue, NativeFunction, js_string,
    object::{ObjectInitializer, builtins::JsArray},
    property::Attribute,
};
use cap_std::fs::OpenOptions;

use super::SandboxCaptures;
use crate::Sandbox;

/// Builds the `fs` object with Node-compatible synchronous file APIs.
pub(super) fn build_fs_object(ctx: &mut Context, sandbox: &Arc<Sandbox>) -> boa_engine::JsObject {
    ObjectInitializer::new(ctx)
        .function(
            NativeFunction::from_copy_closure_with_captures(
                read_file_sync,
                SandboxCaptures::new(sandbox),
            ),
            js_string!("readFileSync"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                write_file_sync,
                SandboxCaptures::new(sandbox),
            ),
            js_string!("writeFileSync"),
            2,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                append_file_sync,
                SandboxCaptures::new(sandbox),
            ),
            js_string!("appendFileSync"),
            2,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                exist_sync,
                SandboxCaptures::new(sandbox),
            ),
            js_string!("existsSync"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                readdir_sync,
                SandboxCaptures::new(sandbox),
            ),
            js_string!("readdirSync"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                mkdir_sync,
                SandboxCaptures::new(sandbox),
            ),
            js_string!("mkdirSync"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                unlink_sync,
                SandboxCaptures::new(sandbox),
            ),
            js_string!("unlinkSync"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                rename_sync,
                SandboxCaptures::new(sandbox),
            ),
            js_string!("renameSync"),
            2,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                copy_file_sync,
                SandboxCaptures::new(sandbox),
            ),
            js_string!("copyFileSync"),
            2,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(rm_sync, SandboxCaptures::new(sandbox)),
            js_string!("rmSync"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                stat_sync,
                SandboxCaptures::new(sandbox),
            ),
            js_string!("statSync"),
            1,
        )
        .build()
}

/// Registers the `fs` global object with Node-compatible synchronous file APIs.
pub(super) fn register(ctx: &mut Context, sandbox: &Arc<Sandbox>) {
    let fs = build_fs_object(ctx, sandbox);
    ctx.register_global_property(
        js_string!("fs"),
        fs,
        Attribute::WRITABLE | Attribute::ENUMERABLE | Attribute::CONFIGURABLE,
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_fs_root<'a>(
    sandbox: &Sandbox,
    files: &'a std::sync::RwLockReadGuard<'a, crate::sandbox::FilesConfig>,
) -> JsResult<&'a cap_std::fs::Dir> {
    if !sandbox.fs_is_enabled() {
        return Err(JsNativeError::error()
            .with_message("file system access is not enabled")
            .into());
    }
    files.root.as_ref().ok_or_else(|| {
        JsNativeError::error()
            .with_message("no file system root configured")
            .into()
    })
}

fn string_arg(args: &[JsValue], index: usize, name: &str) -> JsResult<String> {
    args.get(index)
        .and_then(|v| v.as_string())
        .ok_or_else(|| {
            JsNativeError::typ()
                .with_message(format!("{name} must be a string"))
                .into()
        })
        .map(|s| s.to_std_string_lossy())
}

// ---------------------------------------------------------------------------
// fs.readFileSync(path) -> string
// ---------------------------------------------------------------------------

fn read_file_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = string_arg(args, 0, "path")?;
    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    let content = root
        .read_to_string(&path)
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    Ok(JsValue::from(js_string!(content.as_str())))
}

// ---------------------------------------------------------------------------
// fs.writeFileSync(path, data) -> undefined
// ---------------------------------------------------------------------------

fn write_file_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = string_arg(args, 0, "path")?;
    let content = string_arg(args, 1, "data")?;
    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    root.write(&path, content.as_bytes())
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// fs.appendFileSync(path, data) -> undefined
// ---------------------------------------------------------------------------

fn append_file_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = string_arg(args, 0, "path")?;
    let data = string_arg(args, 1, "data")?;
    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    let mut file = root
        .open_with(
            &path,
            OpenOptions::new().write(true).append(true).create(true),
        )
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    file.write_all(data.as_bytes())
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// fs.existsSync(path) -> boolean
// ---------------------------------------------------------------------------

fn exist_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = string_arg(args, 0, "path")?;
    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    let exists = root.try_exists(&path).unwrap_or(false);
    Ok(JsValue::from(exists))
}

// ---------------------------------------------------------------------------
// fs.readdirSync(path) -> string[]
// ---------------------------------------------------------------------------

fn readdir_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = string_arg(args, 0, "path")?;
    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    let entries = root
        .read_dir(&path)
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    let array = JsArray::new(ctx);
    for entry in entries {
        let name = entry
            .map_err(|e| JsNativeError::error().with_message(e.to_string()))?
            .file_name()
            .to_string_lossy()
            .into_owned();
        array
            .push(js_string!(name.as_str()), ctx)
            .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;
    }

    Ok(JsValue::from(array))
}

// ---------------------------------------------------------------------------
// fs.mkdirSync(path, options?) -> undefined
// ---------------------------------------------------------------------------

fn mkdir_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = string_arg(args, 0, "path")?;
    let recursive = args
        .get(1)
        .filter(|v| !v.is_undefined())
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get(js_string!("recursive"), ctx).ok())
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);

    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    if recursive {
        root.create_dir_all(&path)
    } else {
        root.create_dir(&path)
    }
    .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// fs.statSync(path) -> { size, mtimeMs, isFile(), isDirectory() }
// ---------------------------------------------------------------------------

fn stat_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = string_arg(args, 0, "path")?;
    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    let meta = root
        .metadata(&path)
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    let size = meta.len() as f64;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| {
            t.duration_since(cap_std::time::SystemClock::UNIX_EPOCH)
                .ok()
        })
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0);
    let is_file = meta.is_file();
    let is_dir = meta.is_dir();

    let stat = ObjectInitializer::new(ctx)
        .property(js_string!("size"), size, Attribute::all())
        .property(js_string!("mtimeMs"), mtime_ms, Attribute::all())
        .function(
            NativeFunction::from_copy_closure(move |_, _, _| Ok(JsValue::from(is_file))),
            js_string!("isFile"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure(move |_, _, _| Ok(JsValue::from(is_dir))),
            js_string!("isDirectory"),
            0,
        )
        .build();

    Ok(JsValue::from(stat))
}

// ---------------------------------------------------------------------------
// fs.unlinkSync(path) -> undefined
// ---------------------------------------------------------------------------

fn unlink_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = string_arg(args, 0, "path")?;
    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    root.remove_file(&path)
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// fs.renameSync(oldPath, newPath) -> undefined
// ---------------------------------------------------------------------------

fn rename_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    let old_path = string_arg(args, 0, "oldPath")?;
    let new_path = string_arg(args, 1, "newPath")?;
    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    root.rename(&old_path, root, &new_path)
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// fs.copyFileSync(src, dest) -> undefined
// ---------------------------------------------------------------------------

fn copy_file_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    let src = string_arg(args, 0, "src")?;
    let dest = string_arg(args, 1, "dest")?;
    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    root.copy(&src, root, &dest)
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// fs.rmSync(path, options?) -> undefined
//
// options.recursive: true  →  remove a directory tree (like rm -rf)
// options.force: true      →  ignore errors if the path does not exist
// ---------------------------------------------------------------------------

fn rm_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = string_arg(args, 0, "path")?;
    let opts = args
        .get(1)
        .filter(|v| !v.is_undefined())
        .and_then(|v| v.as_object());

    let recursive = opts
        .as_ref()
        .and_then(|o| o.get(js_string!("recursive"), ctx).ok())
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);
    let force = opts
        .as_ref()
        .and_then(|o| o.get(js_string!("force"), ctx).ok())
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);

    let sandbox = captures.sandbox()?;
    let files = sandbox.files.read().unwrap();
    let root = require_fs_root(&sandbox, &files)?;

    let result = if recursive {
        root.remove_dir_all(&path)
    } else {
        root.remove_file(&path)
    };

    match result {
        Ok(()) => {}
        Err(e) if force && e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(JsNativeError::error().with_message(e.to_string()).into()),
    }

    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tempfile::TempDir;

    use super::super::JsContext;
    use crate::Sandbox;

    // The sandbox must be returned: the context only holds weak references.
    fn setup() -> (JsContext, Arc<Sandbox>, TempDir) {
        let dir = TempDir::new().unwrap();
        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_fs(dir.path()).unwrap();
        let ctx = JsContext::new(&sandbox);
        (ctx, sandbox, dir)
    }

    fn eval(ctx: &mut JsContext, code: &str) -> String {
        ctx.eval(code).unwrap().value
    }

    fn eval_err(ctx: &mut JsContext, code: &str) -> String {
        ctx.eval(code).unwrap_err().to_string()
    }

    #[test]
    fn read_file_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("a.txt", "hello world")"#);
        assert_eq!(
            eval(&mut ctx, r#"fs.readFileSync("a.txt")"#),
            r#""hello world""#
        );
        assert_eq!(
            eval(&mut ctx, r#"typeof fs.readFileSync("a.txt")"#),
            r#""string""#
        );
        assert_eq!(eval(&mut ctx, r#"fs.readFileSync("a.txt").length"#), "11");
        assert_eq!(
            eval(&mut ctx, r#"fs.readFileSync("a.txt").toUpperCase()"#),
            r#""HELLO WORLD""#
        );
        eval(
            &mut ctx,
            r#"fs.writeFileSync("data.json", JSON.stringify({a: 1, b: 2}))"#,
        );
        assert_eq!(
            eval(&mut ctx, r#"JSON.parse(fs.readFileSync("data.json")).a"#),
            "1"
        );
        eval(&mut ctx, r#"fs.writeFileSync("empty.txt", "")"#);
        assert_eq!(eval(&mut ctx, r#"fs.readFileSync("empty.txt")"#), r#""""#);
        assert!(eval_err(&mut ctx, r#"fs.readFileSync("nope.txt")"#).contains("JS error"));
        assert!(eval_err(&mut ctx, r#"fs.readFileSync(42)"#).contains("JS error"));
    }

    #[test]
    fn write_file_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        assert_eq!(
            eval(&mut ctx, r#"fs.writeFileSync("a.txt", "hi")"#),
            "undefined"
        );
        eval(&mut ctx, r#"fs.writeFileSync("a.txt", "second")"#);
        assert_eq!(eval(&mut ctx, r#"fs.readFileSync("a.txt")"#), r#""second""#);
    }

    #[test]
    fn append_file_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("log.txt", "line1")"#);
        eval(&mut ctx, r#"fs.appendFileSync("log.txt", "\nline2")"#);
        assert_eq!(
            eval(&mut ctx, r#"fs.readFileSync("log.txt")"#),
            r#""line1\nline2""#
        );
        eval(&mut ctx, r#"fs.appendFileSync("new.txt", "content")"#);
        assert_eq!(
            eval(&mut ctx, r#"fs.readFileSync("new.txt")"#),
            r#""content""#
        );
    }

    #[test]
    fn exist_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("a.txt", "hi")"#);
        assert_eq!(eval(&mut ctx, r#"fs.existsSync("a.txt")"#), "true");
        assert_eq!(eval(&mut ctx, r#"fs.existsSync("nope.txt")"#), "false");
    }

    #[test]
    fn readdir_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        assert_eq!(
            eval(&mut ctx, r#"Array.isArray(fs.readdirSync("."))"#),
            "true"
        );
        eval(&mut ctx, r#"fs.writeFileSync("a.txt", "")"#);
        eval(&mut ctx, r#"fs.writeFileSync("b.txt", "")"#);
        assert_eq!(
            eval(&mut ctx, r#"JSON.stringify(fs.readdirSync(".").sort())"#),
            r#""[\"a.txt\",\"b.txt\"]""#,
        );
    }

    #[test]
    fn mkdir_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        eval(&mut ctx, r#"fs.mkdirSync("subdir")"#);
        assert_eq!(eval(&mut ctx, r#"fs.existsSync("subdir")"#), "true");
        eval(&mut ctx, r#"fs.mkdirSync("a/b/c", { recursive: true })"#);
        assert_eq!(eval(&mut ctx, r#"fs.existsSync("a/b/c")"#), "true");
        assert!(eval_err(&mut ctx, r#"fs.mkdirSync("x/y/z")"#).contains("JS error"));
    }

    #[test]
    fn stat_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("a.txt", "hi")"#);
        assert_eq!(eval(&mut ctx, r#"fs.statSync("a.txt").size"#), "2");
        assert_eq!(eval(&mut ctx, r#"fs.statSync("a.txt").isFile()"#), "true");
        assert_eq!(
            eval(&mut ctx, r#"fs.statSync("a.txt").isDirectory()"#),
            "false"
        );
        assert_eq!(
            eval(&mut ctx, r#"fs.statSync("a.txt").mtimeMs > 0"#),
            "true"
        );
        eval(&mut ctx, r#"fs.mkdirSync("sub")"#);
        assert_eq!(
            eval(&mut ctx, r#"fs.statSync("sub").isDirectory()"#),
            "true"
        );
        assert_eq!(eval(&mut ctx, r#"fs.statSync("sub").isFile()"#), "false");
        assert!(eval_err(&mut ctx, r#"fs.statSync("nope.txt")"#).contains("JS error"));
    }

    #[test]
    fn unlink_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("a.txt", "hi")"#);
        assert_eq!(eval(&mut ctx, r#"fs.existsSync("a.txt")"#), "true");
        assert_eq!(eval(&mut ctx, r#"fs.unlinkSync("a.txt")"#), "undefined");
        assert_eq!(eval(&mut ctx, r#"fs.existsSync("a.txt")"#), "false");
        assert!(eval_err(&mut ctx, r#"fs.unlinkSync("nope.txt")"#).contains("JS error"));
    }

    #[test]
    fn rename_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("old.txt", "content")"#);
        assert_eq!(
            eval(&mut ctx, r#"fs.renameSync("old.txt", "new.txt")"#),
            "undefined"
        );
        assert_eq!(eval(&mut ctx, r#"fs.existsSync("old.txt")"#), "false");
        assert_eq!(
            eval(&mut ctx, r#"fs.readFileSync("new.txt")"#),
            r#""content""#
        );
        assert!(eval_err(&mut ctx, r#"fs.renameSync("nope.txt", "x.txt")"#).contains("JS error"));
    }

    #[test]
    fn copy_file_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("src.txt", "hello")"#);
        assert_eq!(
            eval(&mut ctx, r#"fs.copyFileSync("src.txt", "dst.txt")"#),
            "undefined"
        );
        assert_eq!(
            eval(&mut ctx, r#"fs.readFileSync("src.txt")"#),
            r#""hello""#
        );
        assert_eq!(
            eval(&mut ctx, r#"fs.readFileSync("dst.txt")"#),
            r#""hello""#
        );
        assert!(
            eval_err(&mut ctx, r#"fs.copyFileSync("nope.txt", "dst.txt")"#).contains("JS error")
        );
    }

    #[test]
    fn rm_sync() {
        let (mut ctx, _sandbox, _dir) = setup();
        // remove a file
        eval(&mut ctx, r#"fs.writeFileSync("a.txt", "hi")"#);
        eval(&mut ctx, r#"fs.rmSync("a.txt")"#);
        assert_eq!(eval(&mut ctx, r#"fs.existsSync("a.txt")"#), "false");
        // remove a directory tree recursively
        eval(&mut ctx, r#"fs.mkdirSync("sub/dir", { recursive: true })"#);
        eval(&mut ctx, r#"fs.writeFileSync("sub/dir/f.txt", "x")"#);
        eval(&mut ctx, r#"fs.rmSync("sub", { recursive: true })"#);
        assert_eq!(eval(&mut ctx, r#"fs.existsSync("sub")"#), "false");
        // force: true silences missing-path errors
        assert_eq!(
            eval(&mut ctx, r#"fs.rmSync("nope.txt", { force: true })"#),
            "undefined"
        );
        // without force, missing path is an error
        assert!(eval_err(&mut ctx, r#"fs.rmSync("nope.txt")"#).contains("JS error"));
    }
}
