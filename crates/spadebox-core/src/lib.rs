mod error;
mod js_runtime;
mod sandbox;
mod tool_utils;
pub mod tools;

pub use error::{ToolError, ToolResult};
pub use sandbox::{DomainRule, FilesConfig, HttpConfig, HttpVerb, JsConfig, Sandbox};
pub use tool_utils::AsArc;
pub use tools::{Tool, ToolDef, call_tool, enabled_tools};

/// Registers a native function as a JavaScript global, available to both the
/// persistent REPL session and fresh `js_exec` contexts.
///
/// `params` declares the positional parameter names. When the function is called
/// from JavaScript, positional arguments are mapped to a JSON object
/// `{ "paramName": value, … }` and passed to `func`. The return value is
/// converted back to a JS value, or a JS `Error` is thrown if `func` returns
/// `Err`.
///
/// Returns [`ToolError::PermissionDenied`] if JavaScript has not been enabled.
///
/// # Example
///
/// ```no_run
/// # use std::sync::Arc;
/// # use spadebox_core::{Sandbox, expose_js_func};
/// let sandbox = Arc::new(Sandbox::new());
/// sandbox.enable_js();
/// expose_js_func(&sandbox, "add", ["a", "b"], |args| {
///     let a = args["a"].as_i64().unwrap_or(0);
///     let b = args["b"].as_i64().unwrap_or(0);
///     Ok(serde_json::Value::Number((a + b).into()))
/// }).unwrap();
/// ```
pub fn expose_js_func(
    sandbox: impl AsArc<Sandbox>,
    name: impl Into<String>,
    params: impl IntoIterator<Item = impl Into<String>>,
    func: impl Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync + 'static,
) -> ToolResult<()> {
    let sandbox = sandbox.as_arc();
    if !sandbox.js_is_enabled() {
        return Err(ToolError::PermissionDenied("JS is disabled".to_string()));
    }
    let params: Vec<String> = params.into_iter().map(Into::into).collect();
    sandbox.js.expose_js_func(name.into(), params, Arc::new(func));
    Ok(())
}

use std::sync::Arc;
