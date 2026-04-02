#![deny(clippy::all)]

use std::collections::BTreeMap;

use napi_derive::napi;
use spadebox_core::{
    Sandbox,
    tools::{
        EditFileTool, EditParams, GlobParams, GlobTool, GrepParams, GrepTool, ReadFileTool,
        ReadParams, Tool, WriteFileTool, WriteParams,
    },
};

fn to_napi_err(e: spadebox_core::ToolError) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

/// Tool metadata exposed to JavaScript.
#[napi(object)]
pub struct JsTool {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's parameters, serialized as a JSON string.
    pub input_schema: String,
}

/// Result of a tool call.
///
/// Distinct from a JavaScript exception: a `JsToolResult` is always returned
/// on successful dispatch. Use `is_error` to tell the agent whether the tool
/// succeeded or encountered a domain error (e.g. file not found).
/// A JavaScript exception is only thrown for protocol errors (unknown tool
/// name, malformed params JSON) that indicate a developer mistake.
#[napi(object)]
pub struct JsToolResult {
    /// `true` when the tool encountered a domain error intended for the agent.
    pub is_error: bool,
    /// The tool's output (success) or error message (tool-level error).
    pub output: String,
}

/// Internal registry entry — static metadata for one tool.
struct ToolEntry {
    name: &'static str,
    description: &'static str,
    /// Pre-serialized JSON Schema string.
    schema: String,
}

fn make_tool_entry<T: Tool>() -> ToolEntry {
    ToolEntry {
        name: T::NAME,
        description: T::DESCRIPTION,
        schema: serde_json::to_string(&T::schema()).expect("schema serialization is infallible"),
    }
}

fn build_tools() -> BTreeMap<&'static str, ToolEntry> {
    let mut map = BTreeMap::new();
    for entry in [
        make_tool_entry::<EditFileTool>(),
        make_tool_entry::<GlobTool>(),
        make_tool_entry::<GrepTool>(),
        make_tool_entry::<ReadFileTool>(),
        make_tool_entry::<WriteFileTool>(),
    ] {
        map.insert(entry.name, entry);
    }
    map
}

#[napi]
pub struct SpadeBox {
    inner: Sandbox,
    tools: BTreeMap<&'static str, ToolEntry>,
}

#[napi]
impl SpadeBox {
    #[napi(constructor)]
    pub fn new(root: String) -> napi::Result<Self> {
        Sandbox::new(&root)
            .map(|inner| SpadeBox {
                inner,
                tools: build_tools(),
            })
            .map_err(to_napi_err)
    }

    /// Returns metadata for all available tools, ordered by name.
    #[napi]
    pub fn tools(&self) -> Vec<JsTool> {
        self.tools
            .values()
            .map(|entry| JsTool {
                name: entry.name.to_string(),
                description: entry.description.to_string(),
                input_schema: entry.schema.clone(),
            })
            .collect()
    }

    /// Call a tool by name, passing its parameters as a JSON string.
    ///
    /// Throws a JavaScript exception on protocol errors (unknown tool name or
    /// malformed params JSON). Returns a [`JsToolResult`] in all other cases —
    /// check `isError` to distinguish tool success from tool-level errors.
    #[napi]
    pub async fn call_tool(&self, name: String, params_json: String) -> napi::Result<JsToolResult> {
        match spadebox_core::call_tool(&self.inner, &name, params_json).await {
            Err(protocol_err) => Err(napi::Error::from_reason(protocol_err)),
            Ok(Ok(output)) => Ok(JsToolResult { is_error: false, output }),
            Ok(Err(tool_err)) => Ok(JsToolResult { is_error: true, output: tool_err.to_string() }),
        }
    }

    #[napi]
    pub async fn read_file(&self, path: String) -> napi::Result<String> {
        ReadFileTool::run(&self.inner, ReadParams { path })
            .await
            .map_err(to_napi_err)
    }

    #[napi]
    pub async fn write_file(
        &self,
        path: String,
        content: Option<String>,
        create_dirs: Option<bool>,
    ) -> napi::Result<String> {
        WriteFileTool::run(
            &self.inner,
            WriteParams {
                path,
                content: content.unwrap_or_default(),
                create_dirs: create_dirs.unwrap_or(false),
            },
        )
        .await
        .map_err(to_napi_err)
    }

    #[napi]
    pub async fn glob(&self, pattern: String) -> napi::Result<String> {
        GlobTool::run(&self.inner, GlobParams { pattern })
            .await
            .map_err(to_napi_err)
    }

    #[napi]
    pub async fn grep(
        &self,
        pattern: String,
        glob: Option<String>,
        context_lines: Option<u32>,
    ) -> napi::Result<String> {
        GrepTool::run(
            &self.inner,
            GrepParams {
                pattern,
                glob,
                context_lines: context_lines.unwrap_or(0),
            },
        )
        .await
        .map_err(to_napi_err)
    }

    #[napi]
    pub async fn edit_file(
        &self,
        path: String,
        old_string: String,
        new_string: String,
        replace_all: Option<bool>,
    ) -> napi::Result<String> {
        EditFileTool::run(
            &self.inner,
            EditParams {
                path,
                old_string,
                new_string,
                replace_all: replace_all.unwrap_or(false),
            },
        )
        .await
        .map_err(to_napi_err)
    }
}
