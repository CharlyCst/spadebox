use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Sandbox, ToolResult};

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

    async fn run(sandbox: &Sandbox, params: Self::Params) -> ToolResult<String> {
        sandbox.js.repl_eval(params.code).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn js_sandbox() -> Sandbox {
        let mut sandbox = Sandbox::new();
        sandbox.js.enable();
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
    async fn disabled_returns_permission_error() {
        let sandbox = Sandbox::new(); // js not enabled
        let err = JsReplTool::run(&sandbox, JsReplParams { code: "1".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, crate::ToolError::PermissionDenied(_)));
    }
}
