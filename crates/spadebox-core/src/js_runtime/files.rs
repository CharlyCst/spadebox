use std::sync::Arc;

use boa_engine::{
    js_string,
    object::ObjectInitializer,
    property::Attribute,
    Context, JsNativeError, JsResult, JsValue, NativeFunction,
};

use crate::Sandbox;
use super::SandboxCaptures;

/// Registers the `fs` global object with `readFileSync` and `writeFileSync`.
pub(super) fn register(ctx: &mut Context, sandbox: Arc<Sandbox>) {
    let fs = ObjectInitializer::new(ctx)
        .function(
            NativeFunction::from_copy_closure_with_captures(
                read_file_sync,
                SandboxCaptures {
                    sandbox: Arc::clone(&sandbox),
                },
            ),
            js_string!("readFileSync"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                write_file_sync,
                SandboxCaptures { sandbox },
            ),
            js_string!("writeFileSync"),
            2,
        )
        .build();

    ctx.register_global_property(
        js_string!("fs"),
        fs,
        Attribute::WRITABLE | Attribute::ENUMERABLE | Attribute::CONFIGURABLE,
    )
    .unwrap();
}

fn read_file_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = args
        .get(0)
        .and_then(|v| v.as_string())
        .ok_or_else(|| JsNativeError::typ().with_message("path must be a string"))?
        .to_std_string_lossy();

    let sandbox = &captures.sandbox;
    if !sandbox.fs_is_enabled() {
        return Err(JsNativeError::error()
            .with_message("file system access is not enabled")
            .into());
    }

    let files = sandbox.files.read().unwrap();
    let root = files
        .root
        .as_ref()
        .ok_or_else(|| JsNativeError::error().with_message("no file system root configured"))?;

    let content = root
        .read_to_string(&path)
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    Ok(JsValue::from(js_string!(content.as_str())))
}

fn write_file_sync(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    let path = args
        .get(0)
        .and_then(|v| v.as_string())
        .ok_or_else(|| JsNativeError::typ().with_message("path must be a string"))?
        .to_std_string_lossy();
    let content = args
        .get(1)
        .and_then(|v| v.as_string())
        .ok_or_else(|| JsNativeError::typ().with_message("content must be a string"))?
        .to_std_string_lossy();

    let sandbox = &captures.sandbox;
    if !sandbox.fs_is_enabled() {
        return Err(JsNativeError::error()
            .with_message("file system access is not enabled")
            .into());
    }

    let files = sandbox.files.read().unwrap();
    let root = files
        .root
        .as_ref()
        .ok_or_else(|| JsNativeError::error().with_message("no file system root configured"))?;

    root.write(&path, content.as_bytes())
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

    Ok(JsValue::undefined())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::Sandbox;
    use super::super::JsContext;

    fn setup() -> (JsContext, TempDir) {
        let dir = TempDir::new().unwrap();
        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_fs(dir.path()).unwrap();
        let ctx = JsContext::new(sandbox);
        (ctx, dir)
    }

    fn eval(ctx: &mut JsContext, code: &str) -> String {
        ctx.eval(code).unwrap()
    }

    fn eval_err(ctx: &mut JsContext, code: &str) -> String {
        ctx.eval(code).unwrap_err().to_string()
    }

    #[test]
    fn write_returns_undefined() {
        let (mut ctx, _dir) = setup();
        assert_eq!(eval(&mut ctx, r#"fs.writeFileSync("a.txt", "hi")"#), "undefined");
    }

    #[test]
    fn read_after_write() {
        let (mut ctx, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("a.txt", "hello world")"#);
        assert_eq!(eval(&mut ctx, r#"fs.readFileSync("a.txt")"#), r#""hello world""#);
    }

    #[test]
    fn result_is_a_string() {
        let (mut ctx, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("a.txt", "hello world")"#);
        assert_eq!(eval(&mut ctx, r#"typeof fs.readFileSync("a.txt")"#), r#""string""#);
    }

    #[test]
    fn string_operations_on_result() {
        let (mut ctx, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("a.txt", "hello world")"#);
        assert_eq!(eval(&mut ctx, r#"fs.readFileSync("a.txt").length"#), "11");
        assert_eq!(eval(&mut ctx, r#"fs.readFileSync("a.txt").toUpperCase()"#), r#""HELLO WORLD""#);
    }

    #[test]
    fn json_roundtrip() {
        let (mut ctx, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("data.json", JSON.stringify({a: 1, b: 2}))"#);
        assert_eq!(eval(&mut ctx, r#"JSON.parse(fs.readFileSync("data.json")).a"#), "1");
    }

    #[test]
    fn empty_file_roundtrip() {
        let (mut ctx, _dir) = setup();
        eval(&mut ctx, r#"fs.writeFileSync("empty.txt", "")"#);
        assert_eq!(eval(&mut ctx, r#"fs.readFileSync("empty.txt")"#), r#""""#);
    }

    #[test]
    fn missing_file_throws() {
        let (mut ctx, _dir) = setup();
        let err = eval_err(&mut ctx, r#"fs.readFileSync("nope.txt")"#);
        assert!(err.contains("JS error"), "unexpected error: {err}");
    }

    #[test]
    fn wrong_arg_type_throws() {
        let (mut ctx, _dir) = setup();
        let err = eval_err(&mut ctx, r#"fs.readFileSync(42)"#);
        assert!(err.contains("JS error"), "unexpected error: {err}");
    }
}
