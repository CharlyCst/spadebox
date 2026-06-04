use std::sync::Arc;

use spadebox_core::{
    DomainRule, HttpVerb, Sandbox, enabled_tools,
    tools::{
        DEFAULT_MAX_MATCHES, DEFAULT_MAX_RESULTS, EditFileTool, EditParams, FetchParams, FetchTool,
        GlobParams, GlobTool, GrepParams, GrepTool, JsExecParams, JsExecTool, JsReplParams,
        JsReplTool, MoveParams, MoveTool, ReadFileTool, ReadParams, Tool, WriteFileTool,
        WriteParams,
    },
};

pub use spadebox_core::ToolError;

/// Error type returned by SpadeBox operations.
///
/// `Error::Tool` carries a [`ToolError`] from tool execution — a domain error the
/// agent should handle (e.g. file not found). `Error::Protocol` signals a
/// developer mistake in a [`SpadeBox::call_tool`] call (unknown tool name or
/// malformed JSON).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error("{0}")]
    Protocol(String),
}

/// Metadata for a single tool exposed by SpadeBox.
#[derive(Debug, Clone)]
pub struct SbTool {
    /// Canonical tool name used in [`SpadeBox::call_tool`].
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema for the tool's parameters, serialized as a JSON string.
    pub input_schema: String,
}

/// Result of a [`SpadeBox::call_tool`] dispatch.
///
/// Distinct from an [`Error`]: `SbToolResult` is always returned when dispatch
/// succeeds. Use `is_error` to tell the agent whether the tool itself succeeded
/// or encountered a domain error (e.g. file not found). An [`Error`] is only
/// returned for protocol errors (unknown tool name, malformed params JSON) that
/// indicate a developer mistake.
#[derive(Debug)]
pub struct SbToolResult {
    /// `true` when the tool encountered a domain error intended for the agent.
    pub is_error: bool,
    /// The tool's output (success) or error message (tool-level error).
    pub output: String,
}

/// Sandboxed tools for AI agents.
///
/// All tools are disabled by default. Use the builder methods to enable tool
/// groups and configure the sandbox, then call tools through the convenience
/// methods or the generic [`call_tool`](SpadeBox::call_tool) dispatcher.
///
/// # Example
///
/// ```no_run
/// # #[tokio::main]
/// # async fn main() -> Result<(), spadebox::Error> {
/// use spadebox::SpadeBox;
///
/// let sb = SpadeBox::new()
///     .enable_files("/path/to/sandbox")?
///     .enable_http()
///     .allow("api.example.com", &["GET", "POST"])?;
///
/// let content = sb.read_file("README.md", None, None, None).await?;
/// # Ok(())
/// # }
/// ```
pub struct SpadeBox {
    inner: Arc<Sandbox>,
}

impl Default for SpadeBox {
    fn default() -> Self {
        Self::new()
    }
}

impl SpadeBox {
    /// Create a new SpadeBox instance with all tools disabled.
    ///
    /// Call [`enable_files`](Self::enable_files) to enable filesystem tools,
    /// [`enable_http`](Self::enable_http) for HTTP fetching, and
    /// [`enable_js`](Self::enable_js) for JavaScript execution.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Sandbox::new()),
        }
    }

    /// Enable filesystem tools with `path` as the sandbox root.
    ///
    /// All file-system operations are restricted to this directory.
    /// Returns `self` for chaining. Fails if `path` cannot be opened.
    pub fn enable_files(self, path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        self.inner.enable_fs(path)?;
        Ok(self)
    }

    /// Enable HTTP fetching.
    ///
    /// After enabling, use [`allow`](Self::allow) to add domain rules
    /// controlling which hosts and HTTP verbs are permitted.
    /// Returns `self` for chaining.
    pub fn enable_http(self) -> Self {
        self.inner.enable_http();
        self
    }

    /// Enable the JavaScript tools.
    ///
    /// Once enabled, the JS REPL session persists across calls: variables and
    /// functions defined in one [`js_repl`](Self::js_repl) call are available
    /// in subsequent ones. Returns `self` for chaining.
    pub fn enable_js(self) -> Self {
        self.inner.enable_js();
        self
    }

    /// Set the `User-Agent` header sent with every HTTP request.
    ///
    /// Defaults to `"spadebox/0.0.0 (AI-agent)"`. Returns `self` for chaining.
    pub fn set_user_agent(self, user_agent: impl Into<String>) -> Self {
        self.inner.set_user_agent(user_agent);
        self
    }

    /// Register a credential and return an opaque token.
    ///
    /// The token is deterministic: the same `name` always produces the same token
    /// across process restarts. Security relies on the `domains` allowlist — the
    /// credential is substituted only when the fetch target matches one of the
    /// supplied domain patterns (same syntax as [`allow`](Self::allow)).
    ///
    /// Pass the returned token as a header value (e.g. `Bearer <token>`);
    /// SpadeBox substitutes the real credential at fetch time for matching hosts.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), spadebox::Error> {
    /// use spadebox::SpadeBox;
    ///
    /// let sb = SpadeBox::new().enable_http().allow("api.github.com", &["GET"])?;
    /// let token = sb.add_credential("github-token", "secret", ["api.github.com"]);
    /// // token is something like "SPADB-a3f7..."
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_credential(
        &self,
        name: impl Into<String>,
        value: impl Into<String>,
        domains: impl IntoIterator<Item = impl Into<String>>,
    ) -> String {
        self.inner.add_credential(name, value, domains)
    }

    /// Add a domain rule permitting the given HTTP verbs for `pattern`.
    ///
    /// `pattern` may be an exact hostname (`"api.example.com"`), a wildcard
    /// subdomain (`"*.example.com"`), or a catch-all (`"*"`). When multiple
    /// rules match a request, the most specific one wins (longest literal suffix).
    ///
    /// `verbs` is a slice of HTTP method strings (e.g. `&["GET", "POST"]`).
    ///
    /// Returns `self` for chaining. Fails if `pattern` is invalid or any verb
    /// is unrecognised.
    pub fn allow(self, pattern: impl Into<String>, verbs: &[&str]) -> Result<Self, Error> {
        let parsed_verbs = verbs
            .iter()
            .map(|v| {
                HttpVerb::parse(&v.to_uppercase())
                    .ok_or_else(|| ToolError::InvalidPattern(format!("unknown HTTP verb '{v}'")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let rule = DomainRule::new(pattern, parsed_verbs)?;
        self.inner.allow(rule);
        Ok(self)
    }

    /// Return metadata for all currently enabled tools.
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
    /// Returns `Err(Error::Protocol)` for unknown tool names or malformed JSON
    /// — these indicate a developer mistake. Returns `Ok(SbToolResult)` in all
    /// other cases — check `is_error` to distinguish tool success from
    /// tool-level errors intended for the agent.
    pub async fn call_tool(&self, name: &str, params_json: &str) -> Result<SbToolResult, Error> {
        match spadebox_core::call_tool(&self.inner, name, params_json.to_owned()).await {
            Err(protocol_err) => Err(Error::Protocol(protocol_err)),
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
    /// `path` must be relative to the sandbox root (e.g. `"src/main.rs"`).
    /// `offset` is a 1-indexed line number to start reading from (default: 1).
    /// `limit` caps the number of lines returned.
    /// `max_bytes` caps the number of bytes returned (default: 20 000). Pass `Some(0)` to disable.
    pub async fn read_file(
        &self,
        path: &str,
        offset: Option<u64>,
        limit: Option<u64>,
        max_bytes: Option<u64>,
    ) -> Result<String, ToolError> {
        ReadFileTool::run(
            &self.inner,
            ReadParams {
                path: path.to_owned(),
                offset,
                limit,
                max_bytes,
            },
        )
        .await
    }

    /// Write text content to a file.
    ///
    /// `path` must be relative to the sandbox root (e.g. `"src/main.rs"`).
    /// Creates the file if it does not exist, or overwrites it entirely if it does.
    /// If the file already exists, it must be read first — attempting to overwrite
    /// without a prior [`read_file`](Self::read_file) call will return an error.
    /// Set `create_dirs` to `true` to create any missing intermediate directories.
    /// To create a directory without writing a file, end `path` with `'/'`
    /// (e.g. `"src/utils/"`) — `content` is ignored in that case.
    pub async fn write_file(
        &self,
        path: &str,
        content: Option<&str>,
        create_dirs: bool,
    ) -> Result<String, ToolError> {
        WriteFileTool::run(
            &self.inner,
            WriteParams {
                path: path.to_owned(),
                content: content.unwrap_or_default().to_owned(),
                create_dirs,
            },
        )
        .await
    }

    /// Replace text within a file.
    ///
    /// Finds the exact string `old_string` in the file at `path` and replaces
    /// it with `new_string`. By default `old_string` must appear exactly once —
    /// include enough surrounding context to make it unique. Set `replace_all`
    /// to `true` to replace every occurrence instead.
    ///
    /// Always read the file before editing to ensure `old_string` matches the
    /// current content exactly.
    pub async fn edit_file(
        &self,
        path: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Result<String, ToolError> {
        EditFileTool::run(
            &self.inner,
            EditParams {
                path: path.to_owned(),
                old_string: old_string.to_owned(),
                new_string: new_string.to_owned(),
                replace_all,
            },
        )
        .await
    }

    /// Move or rename a file or directory, or delete it.
    ///
    /// `src` is the source path relative to the sandbox root. `dst` is the
    /// destination path; pass `None` when deleting.
    /// Set `overwrite` to `true` to replace an existing destination.
    /// Set `delete` to `true` (with no `dst`) to delete `src` instead of moving it.
    /// Set `create_dirs` to `true` to create any missing intermediate directories
    /// for the destination.
    pub async fn move_path(
        &self,
        src: &str,
        dst: Option<&str>,
        overwrite: bool,
        delete: bool,
        create_dirs: bool,
    ) -> Result<String, ToolError> {
        MoveTool::run(
            &self.inner,
            MoveParams {
                src: src.to_owned(),
                dst: dst.map(str::to_owned),
                overwrite,
                delete,
                create_dirs,
            },
        )
        .await
    }

    /// List files matching a glob pattern.
    ///
    /// Returns a newline-separated list of relative file paths sorted
    /// alphabetically. Use `"**"` to match across directories
    /// (e.g. `"**/*.rs"` finds all Rust files, `"src/**/*.ts"` finds TypeScript
    /// files under `src/`).
    pub async fn glob(&self, pattern: &str) -> Result<String, ToolError> {
        GlobTool::run(
            &self.inner,
            GlobParams {
                pattern: pattern.to_owned(),
                max_results: DEFAULT_MAX_RESULTS,
            },
        )
        .await
    }

    /// Search file contents for a regex pattern.
    ///
    /// Returns matching lines with their file path and line number.
    /// Use `glob` to restrict the search to specific file types
    /// (e.g. `"**/*.rs"`). Use `context_lines` to include N surrounding lines
    /// around each match.
    pub async fn grep(
        &self,
        pattern: &str,
        glob: Option<&str>,
        context_lines: u32,
    ) -> Result<String, ToolError> {
        GrepTool::run(
            &self.inner,
            GrepParams {
                pattern: pattern.to_owned(),
                glob: glob.map(str::to_owned),
                context_lines,
                max_matches: DEFAULT_MAX_MATCHES,
            },
        )
        .await
    }

    /// Perform an HTTP request and return the response body as text.
    ///
    /// HTTP must be enabled first via [`enable_http`](Self::enable_http). The
    /// `url` must use the `http` or `https` scheme. `method` is
    /// case-insensitive (e.g. `"GET"`, `"POST"`). Pass `body` for methods that
    /// send a request body (POST, PUT, PATCH). When `raw` is `false` (default),
    /// HTML responses are converted to Markdown. `max_bytes` caps the number of
    /// bytes returned (default: 20 000). Pass `Some(0)` to disable.
    pub async fn fetch(
        &self,
        url: &str,
        method: &str,
        body: Option<&str>,
        raw: bool,
        max_bytes: Option<u64>,
    ) -> Result<String, ToolError> {
        FetchTool::run(
            &self.inner,
            FetchParams {
                url: url.to_owned(),
                method: method.to_owned(),
                body: body.map(str::to_owned),
                raw,
                max_bytes,
            },
        )
        .await
    }

    /// Register a native function as a JavaScript global, available to both the
    /// persistent REPL session and fresh `js_exec` contexts.
    ///
    /// `params` declares the positional parameter names. When the function is
    /// called from JavaScript, positional arguments are mapped to a JSON object
    /// `{ "paramName": value, … }` and passed to `func`. The return value is
    /// converted back to a JS value, or a JS `Error` is thrown if `func` returns
    /// `Err`. Requires [`enable_js`](Self::enable_js) to have been called first.
    pub fn expose_js_func(
        &self,
        name: impl Into<String>,
        params: impl IntoIterator<Item = impl Into<String>>,
        func: impl Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync + 'static,
    ) -> Result<(), ToolError> {
        self.inner.expose_js_func(name, params, func)
    }

    /// Evaluate JavaScript code and return the result as a string.
    ///
    /// The session is persistent: variables and functions defined in one call
    /// are available in subsequent calls. Requires [`enable_js`](Self::enable_js)
    /// to have been called first.
    pub async fn js_repl(&self, code: &str) -> Result<String, ToolError> {
        JsReplTool::run(
            &self.inner,
            JsReplParams {
                code: code.to_owned(),
            },
        )
        .await
    }

    /// Execute a JavaScript file in a fresh runtime.
    ///
    /// `path` must be relative to the sandbox root (e.g. `"scripts/setup.js"`).
    /// Each call starts from a clean context — no state is shared with the JS
    /// REPL or other `js_exec` calls. Requires both [`enable_js`](Self::enable_js)
    /// and [`enable_files`](Self::enable_files). Returns an empty string on
    /// success, or fails if the script throws.
    pub async fn js_exec(&self, path: &str) -> Result<String, ToolError> {
        JsExecTool::run(
            &self.inner,
            JsExecParams {
                path: path.to_owned(),
            },
        )
        .await
    }
}
