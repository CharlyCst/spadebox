use schemars::JsonSchema;
use serde::Deserialize;

use crate::tool_utils::AsArc;
use crate::{Sandbox, ToolResult};

mod edit;
pub(crate) mod fetch;
mod glob;
mod grep;
mod js_exec;
mod js_repl;
mod r#move;
mod read;
mod write;

pub use edit::{EditFileTool, EditParams};
pub use fetch::{FetchParams, FetchTool};
pub use glob::{DEFAULT_MAX_RESULTS, GlobParams, GlobTool};
pub use grep::{DEFAULT_MAX_MATCHES, GrepParams, GrepTool};
pub use js_exec::{JsExecParams, JsExecTool};
pub use js_repl::{JsReplParams, JsReplTool};
pub use r#move::{MoveParams, MoveTool};
pub use read::{ReadFileTool, ReadParams};
pub use write::{WriteFileTool, WriteParams};

/// A sandboxed tool that can be exposed through any interface (MCP, Python, JS, …).
///
/// Implementors define their own strongly-typed `Params`, carry their own `NAME`
/// and `DESCRIPTION`, and provide the async `run` logic. Interface crates
/// (spadebox-mcp, etc.) use these constants and call `run` — they add no logic of
/// their own.
pub trait Tool {
    /// Strongly-typed parameters, deserializable from JSON and self-describing via
    /// JSON Schema so every interface can expose an accurate schema without
    /// duplicating the type definition.
    type Params: for<'de> Deserialize<'de> + JsonSchema;

    /// Canonical tool name exposed to callers (e.g. `"read_file"`).
    const NAME: &'static str;

    /// Human-readable description of what the tool does.
    const DESCRIPTION: &'static str;

    /// Execute the tool against `sandbox` with the given `params`.
    /// Returns a plain UTF-8 string result suitable for wrapping in any
    /// interface's response type.
    fn run(
        sandbox: impl AsArc<Sandbox> + Send,
        params: Self::Params,
    ) -> impl Future<Output = ToolResult<String>> + Send;

    /// JSON Schema for this tool's parameters, generated from the `Params` type.
    fn schema() -> serde_json::Value
    where
        Self: Sized,
    {
        serde_json::to_value(schemars::schema_for!(Self::Params))
            .expect("schema serialization is infallible")
    }

    /// Returns erased metadata for this tool.
    fn def() -> ToolDef
    where
        Self: Sized,
    {
        ToolDef {
            name: Self::NAME,
            description: Self::DESCRIPTION,
            schema: Self::schema(),
        }
    }

    /// Deserialize params from a JSON string and run the tool.
    ///
    /// Returns `Err(String)` if `params_json` cannot be deserialized — this is a
    /// **protocol error** for the developer. Returns `Ok(ToolResult)` otherwise,
    /// where the inner result carries any **tool-level error** intended for the agent.
    fn call_json(
        sandbox: impl AsArc<Sandbox> + Send,
        params_json: String,
    ) -> impl Future<Output = Result<ToolResult<String>, String>> + Send
    where
        Self: Sized,
    {
        async move {
            let params: Self::Params =
                serde_json::from_str(&params_json).map_err(|e| e.to_string())?;
            Ok(Self::run(sandbox, params).await)
        }
    }
}

/// Erased metadata for a single tool, independent of its `Params` type.
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: serde_json::Value,
}

/// Returns metadata for every tool that is currently enabled in `sandbox`.
pub fn enabled_tools(sandbox: &Sandbox) -> Vec<ToolDef> {
    let mut tools = Vec::new();
    if sandbox.fs_is_enabled() {
        tools.push(ReadFileTool::def());
        tools.push(WriteFileTool::def());
        tools.push(EditFileTool::def());
        tools.push(MoveTool::def());
        tools.push(GlobTool::def());
        tools.push(GrepTool::def());
    }
    if sandbox.http_is_enabled() {
        tools.push(FetchTool::def());
    }
    if sandbox.js_is_enabled() {
        tools.push(JsReplTool::def());
    }
    if sandbox.js_is_enabled() && sandbox.fs_is_enabled() {
        tools.push(JsExecTool::def());
    }
    tools
}

/// Dispatch a tool call by name, deserializing params from a JSON string.
///
/// - `Err(String)` — protocol error (unknown tool name or malformed params JSON).
/// - `Ok(Ok(output))` — tool ran successfully.
/// - `Ok(Err(e))` — tool ran but produced an error intended for the agent.
pub async fn call_tool(
    sandbox: impl AsArc<Sandbox> + Send,
    name: &str,
    params_json: String,
) -> Result<ToolResult<String>, String> {
    let sandbox = sandbox.as_arc();
    match name {
        ReadFileTool::NAME if sandbox.fs_is_enabled() => {
            ReadFileTool::call_json(sandbox, params_json).await
        }
        WriteFileTool::NAME if sandbox.fs_is_enabled() => {
            WriteFileTool::call_json(sandbox, params_json).await
        }
        EditFileTool::NAME if sandbox.fs_is_enabled() => {
            EditFileTool::call_json(sandbox, params_json).await
        }
        GlobTool::NAME if sandbox.fs_is_enabled() => {
            GlobTool::call_json(sandbox, params_json).await
        }
        GrepTool::NAME if sandbox.fs_is_enabled() => {
            GrepTool::call_json(sandbox, params_json).await
        }
        MoveTool::NAME if sandbox.fs_is_enabled() => {
            MoveTool::call_json(sandbox, params_json).await
        }
        FetchTool::NAME if sandbox.http_is_enabled() => {
            FetchTool::call_json(sandbox, params_json).await
        }
        JsReplTool::NAME if sandbox.js_is_enabled() => {
            JsReplTool::call_json(sandbox, params_json).await
        }
        JsExecTool::NAME if sandbox.js_is_enabled() && sandbox.fs_is_enabled() => {
            JsExecTool::call_json(sandbox, params_json).await
        }
        name => Err(format!("unknown tool: {name}")),
    }
}
