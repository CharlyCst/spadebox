use boa_engine::{Context, Source};

use crate::{ToolError, ToolResult};

/// A JavaScript execution context with a persistent session.
///
/// Wraps a Boa [`Context`] and exposes a simple [`eval`](JsContext::eval) method.
/// All state — variables, functions, loaded modules — is preserved across calls,
/// giving tools a REPL-like experience.
///
/// `JsContext` is the single place that imports `boa_engine`; callers never
/// interact with Boa types directly.
pub struct JsContext {
    ctx: Context,
}

impl JsContext {
    /// Creates a new `JsContext` with all runtime APIs registered.
    ///
    /// Future additions will include:
    /// - `console.log` / `console.error`
    /// - Sandboxed file I/O backed by `cap-std`
    /// - `fetch` wired through the sandbox's [`crate::sandbox::HttpConfig`]
    pub fn new() -> Self {
        let ctx = Context::default();
        // TODO: register runtime APIs here
        Self { ctx }
    }

    /// Evaluates `code` and returns the result as a string.
    pub fn eval(&mut self, code: &str) -> ToolResult<String> {
        self.ctx
            .eval(Source::from_bytes(code.as_bytes()))
            .map(|v| v.display().to_string())
            .map_err(|e| ToolError::JsError(e.to_string()))
    }
}
