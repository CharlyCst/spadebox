use std::io::Read;
use std::path::Path;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::js_runtime::JsContext;
use crate::tool_utils as fs_utils;
use crate::{AsArc, Sandbox, ToolError, ToolResult, sandbox::map_io_err};

use super::Tool;

pub struct JsExecTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JsExecParams {
    /// Path to the JavaScript file to execute.
    pub path: String,
}

impl Tool for JsExecTool {
    type Params = JsExecParams;

    const NAME: &'static str = "js_exec";

    const DESCRIPTION: &'static str = "Execute a JavaScript file in a fresh runtime and return \
         an empty string on success, or an error message if the script throws. \
         No state is shared with the JS REPL — each call starts from a clean context.";

    async fn run(sandbox: impl AsArc<Sandbox> + Send, params: Self::Params) -> ToolResult<String> {
        let sandbox = sandbox.as_arc();
        if !sandbox.js_is_enabled() {
            return Err(ToolError::PermissionDenied("JS is disabled".to_string()));
        }
        if !sandbox.fs_is_enabled() {
            return Err(ToolError::PermissionDenied(
                "file system is disabled".to_string(),
            ));
        }

        tokio::task::spawn_blocking(move || {
            let path = fs_utils::normalize_path(&params.path).to_string();
            let mut file = {
                let fs_config = sandbox.files.read().unwrap();
                fs_config
                    .root
                    .as_ref()
                    .expect("Missing sandbox root")
                    .open(&path)
                    .map_err(|e| map_io_err(&path, e))?
            };
            let mut code = String::new();
            file.read_to_string(&mut code).map_err(ToolError::IoError)?;

            let mut ctx = JsContext::new(&sandbox);
            let funcs = sandbox.js.funcs.read().unwrap();
            ctx.register_funcs(&funcs)?;
            drop(funcs);
            ctx.eval_module(&code, Path::new(&path))
                .map(|output| output.console.join("\n"))
        })
        .await
        .map_err(|e| ToolError::JsError(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn setup() -> (tempfile::TempDir, Arc<Sandbox>) {
        let dir = tempfile::TempDir::new().unwrap();
        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_fs(dir.path()).unwrap();
        sandbox.enable_js();
        (dir, sandbox)
    }

    #[tokio::test]
    async fn executes_file_successfully() {
        let (dir, sandbox) = setup();
        std::fs::write(dir.path().join("script.js"), "1 + 1").unwrap();

        let result = JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "script.js".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn js_error_is_surfaced() {
        let (dir, sandbox) = setup();
        std::fs::write(dir.path().join("err.js"), "throw new Error('boom')").unwrap();

        let err = JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "err.js".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ToolError::JsError(_)));
    }

    #[tokio::test]
    async fn missing_file_returns_error() {
        let (_dir, sandbox) = setup();

        let err = JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "nope.js".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ToolError::NotFound(_)));
    }

    #[tokio::test]
    async fn runtime_is_fresh_per_call() {
        let (dir, sandbox) = setup();
        // Define a variable in the first call — it must not bleed into the second.
        std::fs::write(dir.path().join("a.js"), "var x = 42;").unwrap();
        std::fs::write(
            dir.path().join("b.js"),
            "if (typeof x !== 'undefined') throw new Error('leaked');",
        )
        .unwrap();

        JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "a.js".into(),
            },
        )
        .await
        .unwrap();
        JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "b.js".into(),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn fs_accessible_from_script() {
        let (dir, sandbox) = setup();
        std::fs::write(dir.path().join("data.txt"), "hello").unwrap();
        std::fs::write(
            dir.path().join("read.js"),
            r#"var s = fs.readFileSync("data.txt"); if (s !== "hello") throw new Error(s);"#,
        )
        .unwrap();

        JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "read.js".into(),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn disabled_js_returns_permission_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_fs(dir.path()).unwrap();
        // JS not enabled

        let err = JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "x.js".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ToolError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn import_fs_works() {
        let (dir, sandbox) = setup();
        std::fs::write(
            dir.path().join("script.js"),
            r#"import { readFileSync, writeFileSync } from "node:fs";
writeFileSync("out.txt", "hello from module");
const content = readFileSync("out.txt");
console.log(content);"#,
        )
        .unwrap();

        let result = JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "script.js".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(result, "hello from module");
    }

    #[tokio::test]
    async fn top_level_await_works() {
        let (dir, sandbox) = setup();
        std::fs::write(
            dir.path().join("script.js"),
            r#"const x = await Promise.resolve(42);
console.log(x);"#,
        )
        .unwrap();

        let result = JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "script.js".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(result, "42");
    }

    #[tokio::test]
    async fn disabled_fs_returns_permission_error() {
        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_js();
        // FS not enabled

        let err = JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "x.js".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ToolError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn exposed_func_available_in_exec() {
        let (dir, sandbox) = setup();
        sandbox
            .expose_js_func("double", ["n"], |args| {
                let n = args.get("n").and_then(|v| v.as_i64()).unwrap_or(0);
                Ok(serde_json::Value::Number((n * 2).into()))
            })
            .unwrap();

        std::fs::write(
            dir.path().join("use_double.js"),
            r#"var r = double(21); if (r !== 42) throw new Error("got " + r);"#,
        )
        .unwrap();

        JsExecTool::run(
            &sandbox,
            JsExecParams {
                path: "use_double.js".into(),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn import_local_modules() {
        let (dir, sandbox) = setup();

        // flat import
        std::fs::write(dir.path().join("utils.js"), "export const double = x => x * 2;").unwrap();
        std::fs::write(
            dir.path().join("main.js"),
            r#"import { double } from "./utils.js";
console.log(double(21));"#,
        )
        .unwrap();
        let result = JsExecTool::run(&sandbox, JsExecParams { path: "main.js".into() })
            .await
            .unwrap();
        assert_eq!(result, "42");

        // subdirectory import
        std::fs::create_dir(dir.path().join("lib")).unwrap();
        std::fs::write(
            dir.path().join("lib/math.js"),
            "export const add = (a, b) => a + b;",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("sub.js"),
            r#"import { add } from "./lib/math.js";
console.log(add(1, 2));"#,
        )
        .unwrap();
        let result = JsExecTool::run(&sandbox, JsExecParams { path: "sub.js".into() })
            .await
            .unwrap();
        assert_eq!(result, "3");

        // circular imports — ES live bindings ensure `name` is defined by the time greet() runs
        std::fs::write(
            dir.path().join("a.js"),
            r#"import { greet } from "./b.js";
export const name = "world";
greet();"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.js"),
            r#"import { name } from "./a.js";
export function greet() { console.log("hello " + name); }"#,
        )
        .unwrap();
        let result = JsExecTool::run(&sandbox, JsExecParams { path: "a.js".into() })
            .await
            .unwrap();
        assert_eq!(result, "hello world");

        // path traversal is rejected
        std::fs::write(dir.path().join("evil.js"), r#"import "../../etc/passwd";"#).unwrap();
        let err = JsExecTool::run(&sandbox, JsExecParams { path: "evil.js".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::JsError(_)));
    }
}
