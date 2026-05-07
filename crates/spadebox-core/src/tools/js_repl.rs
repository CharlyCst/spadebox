use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{AsArc, Sandbox, ToolResult};

use super::Tool;

pub struct JsReplTool;

/// Parameters for the `js_repl` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct JsReplParams {
    /// JavaScript source code to evaluate.
    ///
    /// The evaluation runs in a persistent session: variables, functions, and
    /// any other state defined in previous calls are available in subsequent ones.
    pub code: String,
}

impl Tool for JsReplTool {
    type Params = JsReplParams;

    const NAME: &'static str = "js_repl";

    const DESCRIPTION: &'static str = "Evaluate JavaScript code and return the result as a string. \
         The session is persistent: variables and functions defined in one call \
         are available in subsequent calls.";

    async fn run(sandbox: impl AsArc<Sandbox> + Send, params: Self::Params) -> ToolResult<String> {
        let sandbox = sandbox.as_arc();
        if !sandbox.js_is_enabled() {
            return Err(crate::ToolError::PermissionDenied(
                "JS REPL is disabled".to_string(),
            ));
        }
        let output = sandbox.js.repl_eval(Arc::clone(&sandbox), params.code).await?;
        if output.console.is_empty() {
            Ok(output.value)
        } else {
            Ok(format!("{}\n{}", output.console.join("\n"), output.value))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn js_sandbox() -> Arc<Sandbox> {
        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_js();
        sandbox
    }

    #[tokio::test]
    async fn evaluates_expression() {
        let sandbox = js_sandbox();
        let result = JsReplTool::run(
            &sandbox,
            JsReplParams {
                code: "1 + 1".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(result, "2");
    }

    #[tokio::test]
    async fn session_is_persistent() {
        let sandbox = js_sandbox();
        JsReplTool::run(
            &sandbox,
            JsReplParams {
                code: "let x = 42;".into(),
            },
        )
        .await
        .unwrap();
        let result = JsReplTool::run(&sandbox, JsReplParams { code: "x".into() })
            .await
            .unwrap();
        assert_eq!(result, "42");
    }

    #[tokio::test]
    async fn js_error_is_surfaced() {
        let sandbox = js_sandbox();
        let err = JsReplTool::run(
            &sandbox,
            JsReplParams {
                code: "throw new Error('oops')".into(),
            },
        )
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("JS error"), "unexpected error: {msg}");
    }

    #[tokio::test]
    async fn read_file_sync() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hello world").unwrap();

        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_fs(dir.path()).unwrap();
        sandbox.enable_js();

        let result = JsReplTool::run(
            &sandbox,
            JsReplParams {
                code: r#"fs.readFileSync("hello.txt")"#.into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(result, "\"hello world\"");
    }

    #[tokio::test]
    async fn write_file_sync() {
        let dir = tempfile::TempDir::new().unwrap();

        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_fs(dir.path()).unwrap();
        sandbox.enable_js();

        JsReplTool::run(
            &sandbox,
            JsReplParams {
                code: r#"fs.writeFileSync("out.txt", "from js")"#.into(),
            },
        )
        .await
        .unwrap();

        let content = std::fs::read_to_string(dir.path().join("out.txt")).unwrap();
        assert_eq!(content, "from js");
    }

    #[tokio::test]
    async fn fs_access_denied_without_enable_fs() {
        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_js();

        let err = JsReplTool::run(
            &sandbox,
            JsReplParams {
                code: r#"fs.readFileSync("x.txt")"#.into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::ToolError::JsError(_)));
    }

    #[tokio::test]
    async fn disabled_returns_permission_error() {
        let sandbox = Arc::new(Sandbox::new()); // js not enabled
        let err = JsReplTool::run(sandbox, JsReplParams { code: "1".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, crate::ToolError::PermissionDenied(_)));
    }
}
