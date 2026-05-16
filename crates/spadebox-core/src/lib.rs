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
/// The function is callable by name from any subsequent `js_repl` or `js_exec`
/// call. Arguments passed from JavaScript are converted to strings; the return
/// value is converted back to a JS string, or a JS `Error` is thrown if the
/// closure returns `Err`.
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
/// expose_js_func(&sandbox, "add", |args| {
///     let a: i64 = args.get(0).and_then(|s| s.parse().ok()).unwrap_or(0);
///     let b: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
///     Ok((a + b).to_string())
/// }).unwrap();
/// ```
pub fn expose_js_func(
    sandbox: impl AsArc<Sandbox>,
    name: impl Into<String>,
    func: impl Fn(Vec<String>) -> Result<String, String> + Send + Sync + 'static,
) -> ToolResult<()> {
    let sandbox = sandbox.as_arc();
    if !sandbox.js_is_enabled() {
        return Err(ToolError::PermissionDenied("JS is disabled".to_string()));
    }
    sandbox.js.expose_js_func(name.into(), Arc::new(func));
    Ok(())
}

use std::sync::Arc;
