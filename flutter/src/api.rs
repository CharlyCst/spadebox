// Doc comments here are surfaced as Dart API documentation by flutter_rust_bridge.
// Use camelCase names in doc comments to match Dart conventions.

use anyhow::anyhow;
use flutter_rust_bridge::frb;
use spadebox_core::{
    DomainRule, HttpVerb, Sandbox, enabled_tools,
    tools::{
        DEFAULT_MAX_MATCHES, DEFAULT_MAX_RESULTS, EditFileTool, EditParams, FetchParams, FetchTool,
        GlobParams, GlobTool, GrepParams, GrepTool, JsExecParams, JsExecTool, JsReplParams,
        JsReplTool, MoveParams, MoveTool, ReadFileTool, ReadParams, Tool, WriteFileTool, WriteParams,
    },
};
use std::collections::HashMap;
use std::sync::Arc;

fn to_anyhow(e: spadebox_core::ToolError) -> anyhow::Error {
    anyhow!("{e}")
}

/// Tool metadata.
pub struct SbTool {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's parameters, serialized as a JSON string.
    pub input_schema: String,
}

/// Result of a tool call.
///
/// Distinct from a Dart exception: an `SbToolResult` is always returned on
/// successful dispatch. Use `isError` to tell the agent whether the tool
/// succeeded or encountered a domain error (e.g. file not found). A Dart
/// exception (`FrbException`) is only thrown for protocol errors (unknown
/// tool name, malformed params JSON) that indicate a developer mistake.
pub struct SbToolResult {
    /// `true` when the tool encountered a domain error intended for the agent.
    pub is_error: bool,
    /// The tool's output (success) or error message (tool-level error).
    pub output: String,
}

/// Main SpadeBox handle. Wraps a sandboxed tool runtime.
///
/// Create with `SpadeBox.new_()`, then call `enableFiles` and/or `enableHttp`
/// before using any tools.
#[frb(opaque)]
pub struct SpadeBox {
    inner: Arc<Sandbox>,
}

impl SpadeBox {
    /// Create a new SpadeBox instance with all tools disabled.
    ///
    /// Call `enableFiles` to enable filesystem tools and `enableHttp` to
    /// enable HTTP fetching.
    #[frb(sync)]
    pub fn new_() -> SpadeBox {
        SpadeBox {
            inner: Arc::new(Sandbox::new()),
        }
    }

    /// Enable filesystem tools with `path` as the sandbox root.
    ///
    /// All file-system operations are restricted to this directory.
    /// Throws if `path` cannot be opened.
    pub fn enable_files(&self, path: String) -> anyhow::Result<()> {
        self.inner.enable_fs(&path).map(|_| ()).map_err(to_anyhow)
    }

    /// Enable HTTP fetching. Chain with `allow` to permit specific domains.
    pub fn enable_http(&self) {
        self.inner.enable_http();
    }

    /// Enable the JavaScript runtime tools (jsRepl / jsExec).
    pub fn enable_js(&self) {
        self.inner.enable_js();
    }

    /// Set the `User-Agent` header sent with every HTTP request.
    pub fn set_user_agent(&self, user_agent: String) {
        self.inner.set_user_agent(user_agent);
    }

    /// Register a credential and return an opaque substitution token.
    ///
    /// Pass the returned token as a header value; SpadeBox substitutes the
    /// real credential at fetch time for requests matching `domains`.
    pub fn add_credential(&self, name: String, value: String, domains: Vec<String>) -> String {
        self.inner.add_credential(name, value, domains)
    }

    /// Add a domain rule permitting the given HTTP `verbs` for `pattern`.
    ///
    /// `pattern` may be an exact hostname (`"api.example.com"`), a wildcard
    /// subdomain (`"*.example.com"`), or a catch-all (`"*"`).
    /// `verbs` are case-insensitive strings (e.g. `["GET", "POST"]`).
    pub fn allow(&self, pattern: String, verbs: Vec<String>) -> anyhow::Result<()> {
        let parsed_verbs = verbs
            .iter()
            .map(|v| {
                HttpVerb::parse(v.to_uppercase().as_str())
                    .ok_or_else(|| anyhow!("unknown HTTP verb '{v}'"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let rule = DomainRule::new(pattern, parsed_verbs).map_err(to_anyhow)?;
        self.inner.allow(rule);
        Ok(())
    }

    /// Returns metadata for all currently enabled tools.
    pub fn tools(&self) -> Vec<SbTool> {
        enabled_tools(&self.inner)
            .into_iter()
            .map(|def| SbTool {
                name: def.name.to_string(),
                description: def.description.to_string(),
                input_schema: serde_json::to_string(&def.schema)
                    .expect("schema serialization is infallible"),
            })
            .collect()
    }

    /// Call a tool by name, passing its parameters as a JSON string.
    ///
    /// Throws on protocol errors (unknown tool name, malformed JSON).
    /// Returns an `SbToolResult` in all other cases — check `isError` to
    /// distinguish tool success from tool-level errors.
    pub async fn call_tool(
        &self,
        name: String,
        params_json: String,
    ) -> anyhow::Result<SbToolResult> {
        match spadebox_core::call_tool(&self.inner, &name, params_json).await {
            Err(protocol_err) => Err(anyhow!(protocol_err)),
            Ok(Ok(output)) => Ok(SbToolResult {
                is_error: false,
                output,
            }),
            Ok(Err(tool_err)) => Ok(SbToolResult {
                is_error: true,
                output: tool_err.to_string(),
            }),
        }
    }

    /// Read the text content of a file.
    ///
    /// `path` is relative to the sandbox root. `offset` is a 1-indexed line
    /// number to start reading from. `limit` caps the number of lines returned.
    /// `maxBytes` caps bytes returned (default 20 000; pass 0 to disable).
    pub async fn read_file(
        &self,
        path: String,
        offset: Option<u32>,
        limit: Option<u32>,
        max_bytes: Option<u32>,
    ) -> anyhow::Result<String> {
        ReadFileTool::run(
            &self.inner,
            ReadParams {
                path,
                offset: offset.map(Into::into),
                limit: limit.map(Into::into),
                max_bytes: max_bytes.map(Into::into),
            },
        )
        .await
        .map_err(to_anyhow)
    }

    /// Write text content to a file.
    ///
    /// `path` is relative to the sandbox root. Creates the file if it does not
    /// exist, or overwrites it entirely if it does. Set `createDirs` to `true`
    /// to create any missing intermediate directories automatically.
    pub async fn write_file(
        &self,
        path: String,
        content: Option<String>,
        create_dirs: Option<bool>,
    ) -> anyhow::Result<String> {
        WriteFileTool::run(
            &self.inner,
            WriteParams {
                path,
                content: content.unwrap_or_default(),
                create_dirs: create_dirs.unwrap_or(false),
            },
        )
        .await
        .map_err(to_anyhow)
    }

    /// List files matching a glob pattern.
    ///
    /// Returns a newline-separated list of relative paths sorted alphabetically.
    pub async fn glob(&self, pattern: String) -> anyhow::Result<String> {
        GlobTool::run(
            &self.inner,
            GlobParams {
                pattern,
                max_results: DEFAULT_MAX_RESULTS,
            },
        )
        .await
        .map_err(to_anyhow)
    }

    /// Search file contents for a regex pattern.
    ///
    /// Returns matching lines with file path and line number. Use `glob` to
    /// restrict to specific file types (e.g. `"**/*.rs"`).
    pub async fn grep(
        &self,
        pattern: String,
        glob: Option<String>,
        context_lines: Option<u32>,
    ) -> anyhow::Result<String> {
        GrepTool::run(
            &self.inner,
            GrepParams {
                pattern,
                glob,
                context_lines: context_lines.unwrap_or(0),
                max_matches: DEFAULT_MAX_MATCHES,
            },
        )
        .await
        .map_err(to_anyhow)
    }

    /// Replace text within a file.
    ///
    /// Finds the exact string `oldString` and replaces it with `newString`.
    /// Set `replaceAll` to `true` to replace every occurrence.
    pub async fn edit_file(
        &self,
        path: String,
        old_string: String,
        new_string: String,
        replace_all: Option<bool>,
    ) -> anyhow::Result<String> {
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
        .map_err(to_anyhow)
    }

    /// Move or rename a file or directory, or delete it.
    ///
    /// `src` is relative to the sandbox root. Pass `dst` to move/rename;
    /// omit it and set `delete` to `true` to delete instead.
    pub async fn move_path(
        &self,
        src: String,
        dst: Option<String>,
        overwrite: Option<bool>,
        delete: Option<bool>,
        create_dirs: Option<bool>,
    ) -> anyhow::Result<String> {
        MoveTool::run(
            &self.inner,
            MoveParams {
                src,
                dst,
                overwrite: overwrite.unwrap_or(false),
                delete: delete.unwrap_or(false),
                create_dirs: create_dirs.unwrap_or(false),
            },
        )
        .await
        .map_err(to_anyhow)
    }

    /// Perform an HTTP request and return the response body as text.
    ///
    /// HTTP must be enabled first via `enableHttp`. `method` is
    /// case-insensitive. HTML responses are converted to Markdown unless
    /// `raw` is `true`.
    pub async fn fetch(
        &self,
        url: String,
        method: String,
        body: Option<String>,
        headers: Option<HashMap<String, String>>,
        raw: Option<bool>,
        max_bytes: Option<u32>,
    ) -> anyhow::Result<String> {
        FetchTool::run(
            &self.inner,
            FetchParams {
                url,
                method,
                body,
                headers,
                raw: raw.unwrap_or(false),
                max_bytes: max_bytes.map(Into::into),
            },
        )
        .await
        .map_err(to_anyhow)
    }

    /// Evaluate JavaScript code in a persistent session.
    ///
    /// Variables and functions defined in one call are available in subsequent
    /// calls. Requires `enableJs` to have been called first.
    pub async fn js_repl(&self, code: String) -> anyhow::Result<String> {
        JsReplTool::run(&self.inner, JsReplParams { code })
            .await
            .map_err(to_anyhow)
    }

    /// Execute a JavaScript file in a fresh runtime.
    ///
    /// `path` is relative to the sandbox root. Each call starts from a clean
    /// context. Requires both `enableJs` and `enableFiles`.
    pub async fn js_exec(&self, path: String) -> anyhow::Result<String> {
        JsExecTool::run(&self.inner, JsExecParams { path })
            .await
            .map_err(to_anyhow)
    }
}
