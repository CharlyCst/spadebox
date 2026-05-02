// Doc comments in this file are surfaced as JavaScript API documentation via
// NAPI-RS. Use camelCase for parameter and field names in doc comments to match
// the JavaScript calling convention (NAPI-RS converts snake_case identifiers to
// camelCase in the generated bindings).

use napi::bindgen_prelude::This;
use napi_derive::napi;
use spadebox_core::{
  DomainRule, HttpVerb, Sandbox, enabled_tools,
  tools::{
    EditFileTool, EditParams, FetchParams, FetchTool, GlobParams, GlobTool, GrepParams, GrepTool,
    JsReplParams, JsReplTool, MoveParams, MoveTool, ReadFileTool, ReadParams, Tool, WriteFileTool,
    WriteParams,
  },
};

fn to_napi_err(e: spadebox_core::ToolError) -> napi::Error {
  napi::Error::from_reason(e.to_string())
}

/// Tool metadata exposed to JavaScript.
#[napi(object)]
pub struct SbTool {
  pub name: String,
  pub description: String,
  /// JSON Schema for the tool's parameters, serialized as a JSON string.
  pub input_schema: String,
}

/// Result of a tool call.
///
/// Distinct from a JavaScript exception: a `SbToolResult` is always returned
/// on successful dispatch. Use `isError` to tell the agent whether the tool
/// succeeded or encountered a domain error (e.g. file not found).
/// A JavaScript exception is only thrown for protocol errors (unknown tool
/// name, malformed params JSON) that indicate a developer mistake.
#[napi(object)]
pub struct SbToolResult {
  /// `true` when the tool encountered a domain error intended for the agent.
  pub is_error: bool,
  /// The tool's output (success) or error message (tool-level error).
  pub output: String,
}

#[napi]
pub struct SpadeBox {
  inner: Sandbox,
}

#[napi]
impl SpadeBox {
  /// Create a new SpadeBox instance with all tools disabled.
  ///
  /// Call `enableFiles` to enable filesystem tools and `enableHttp` to enable
  /// HTTP fetching.
  // `Default` is not meaningful for a NAPI class exposed to JavaScript.
  #[allow(clippy::new_without_default)]
  #[napi(constructor)]
  pub fn new() -> Self {
    Self {
      inner: Sandbox::new(),
    }
  }

  /// Enable filesystem tools with `path` as the sandbox root.
  ///
  /// All file-system operations are restricted to this directory. Returns
  /// `this` for chaining. Throws if `path` cannot be opened.
  #[napi]
  pub fn enable_files<'env>(&mut self, path: String, this: This<'env>) -> napi::Result<This<'env>> {
    self.inner.files.enable(&path).map_err(to_napi_err)?;
    Ok(this)
  }

  /// Returns metadata for all currently enabled tools.
  #[napi]
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

  /// Call a tool by name, passing its parameters as a JSON string (`paramsJson`).
  ///
  /// Throws a JavaScript exception on protocol errors (unknown tool name or
  /// malformed `paramsJson`). Returns a [`SbToolResult`] in all other cases —
  /// check `isError` to distinguish tool success from tool-level errors.
  #[napi]
  pub async fn call_tool(&self, name: String, params_json: String) -> napi::Result<SbToolResult> {
    match spadebox_core::call_tool(&self.inner, &name, params_json).await {
      Err(protocol_err) => Err(napi::Error::from_reason(protocol_err)),
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

  /// Read the text content of a file. Calls the `read_file` tool directly.
  ///
  /// `path` must be relative to the sandbox root (e.g. `'src/main.rs'`).
  /// `offset` is a 1-indexed line number to start reading from (default: 1).
  /// `limit` caps the number of lines returned.
  /// `maxBytes` caps the number of bytes returned (default: 20 000). Pass `0` to disable.
  /// Returns the file's content as a UTF-8 string.
  #[napi]
  pub async fn read_file(
    &self,
    path: String,
    offset: Option<u32>,
    limit: Option<u32>,
    max_bytes: Option<u32>,
  ) -> napi::Result<String> {
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
    .map_err(to_napi_err)
  }

  /// Write text content to a file. Calls the `write_file` tool directly.
  ///
  /// `path` must be relative to the sandbox root (e.g. `'src/main.rs'`).
  /// Creates the file if it does not exist, or overwrites it entirely if it does.
  /// If the file already exists, it must be read first — attempting to overwrite
  /// without a prior `readFile` call will throw an error.
  /// Set `createDirs` to `true` to create any missing intermediate directories
  /// automatically. To create a directory without writing a file, end `path`
  /// with `'/'` (e.g. `'src/utils/'`) — `content` is ignored in that case.
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

  /// List files matching a glob pattern. Calls the `glob` tool directly.
  ///
  /// Returns a newline-separated list of relative file paths sorted
  /// alphabetically. Use `'**'` to match across directories
  /// (e.g. `'**/*.rs'` finds all Rust files, `'src/**/*.ts'` finds TypeScript
  /// files under `src/`).
  #[napi]
  pub async fn glob(&self, pattern: String) -> napi::Result<String> {
    GlobTool::run(&self.inner, GlobParams { pattern })
      .await
      .map_err(to_napi_err)
  }

  /// Search file contents for a regex pattern. Calls the `grep` tool directly.
  ///
  /// Returns matching lines with their file path and line number.
  /// Use `glob` to restrict the search to specific file types
  /// (e.g. `'**/*.rs'`). Use `contextLines` to include N surrounding lines
  /// around each match.
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

  /// Set the `User-Agent` header sent with every HTTP request.
  ///
  /// Defaults to `"spadebox/0.0.0 (AI-agent)"`. Returns `this` for chaining.
  ///
  /// ```js
  /// sb.setUserAgent('myagent/1.0').enableHttp().allow('api.example.com', ['GET']);
  /// ```
  #[napi]
  pub fn set_user_agent<'env>(&mut self, user_agent: String, this: This<'env>) -> This<'env> {
    self.inner.http.set_user_agent(user_agent);
    this
  }

  /// Enable HTTP fetching. Returns `this` for chaining with `allow`.
  ///
  /// ```js
  /// sb.enableHttp().allow('api.example.com', ['GET', 'POST']).allow('*.cdn.example.com', ['GET']);
  /// ```
  #[napi]
  pub fn enable_http<'env>(&mut self, this: This<'env>) -> This<'env> {
    self.inner.http.enable();
    this
  }

  /// Add a domain rule permitting the given HTTP verbs for `pattern`.
  ///
  /// `pattern` may be an exact hostname (`"api.example.com"`), a wildcard
  /// subdomain (`"*.example.com"`), or a catch-all (`"*"`). When multiple rules
  /// match a request, the most specific one wins (longest literal suffix).
  /// Returns `this` for chaining.
  ///
  /// Throws if `pattern` is invalid or any verb is unrecognised.
  #[napi]
  pub fn allow<'env>(
    &mut self,
    pattern: String,
    verbs: Vec<String>,
    this: This<'env>,
  ) -> napi::Result<This<'env>> {
    let parsed_verbs = verbs
      .iter()
      .map(|v| {
        HttpVerb::parse(v.to_uppercase().as_str())
          .ok_or_else(|| napi::Error::from_reason(format!("unknown HTTP verb '{v}'")))
      })
      .collect::<napi::Result<Vec<_>>>()?;
    let rule = DomainRule::new(pattern, parsed_verbs).map_err(to_napi_err)?;
    self.inner.http.allow(rule);
    Ok(this)
  }

  /// Perform an HTTP request and return the response body as text.
  ///
  /// HTTP must be enabled first via `enableHttp`. The `url` must use the `http`
  /// or `https` scheme. `method` is case-insensitive (e.g. `"GET"`, `"POST"`).
  /// Pass `body` for methods that send a request body (POST, PUT, PATCH).
  /// When `raw` is `false` (default), HTML responses are converted to Markdown.
  /// Set `raw` to `true` to receive the raw response body unchanged.
  /// `maxBytes` caps the number of bytes returned (default: 20 000). Pass `0` to disable.
  #[napi]
  pub async fn fetch(
    &self,
    url: String,
    method: String,
    body: Option<String>,
    raw: Option<bool>,
    max_bytes: Option<u32>,
  ) -> napi::Result<String> {
    FetchTool::run(
      &self.inner,
      FetchParams {
        url,
        method,
        body,
        raw: raw.unwrap_or(false),
        max_bytes: max_bytes.map(Into::into),
      },
    )
    .await
    .map_err(to_napi_err)
  }

  /// Enable the JavaScript tools. Returns `this` for chaining.
  ///
  /// Once enabled, the JS REPL session persists across calls: variables and functions
  /// defined in one `jsRepl` call are available in subsequent ones.
  ///
  /// ```js
  /// const sb = new SpadeBox().enableJs();
  /// ```
  #[napi]
  pub fn enable_js<'env>(&mut self, this: This<'env>) -> This<'env> {
    self.inner.js.enable();
    this
  }

  /// Evaluate JavaScript code and return the result as a string.
  ///
  /// The session is persistent: variables and functions defined in one call are
  /// available in subsequent calls. Requires `enableJs` to have been called first.
  #[napi]
  pub async fn js_repl(&self, code: String) -> napi::Result<String> {
    JsReplTool::run(&self.inner, JsReplParams { code })
      .await
      .map_err(to_napi_err)
  }

  /// Move or rename a file or directory, or delete it. Calls the `move` tool directly.
  ///
  /// `src` is the source path relative to the sandbox root. `dst` is the
  /// destination path; omit it (pass `null`) when deleting.
  /// Set `overwrite` to `true` to replace an existing destination. Set `delete` to `true`
  /// (with no `dst`) to delete `src` instead of moving it.
  #[napi(js_name = "move")]
  pub async fn mv(
    &self,
    src: String,
    dst: Option<String>,
    overwrite: Option<bool>,
    del: Option<bool>,
  ) -> napi::Result<String> {
    MoveTool::run(
      &self.inner,
      MoveParams {
        src,
        dst,
        overwrite: overwrite.unwrap_or(false),
        delete: del.unwrap_or(false),
      },
    )
    .await
    .map_err(to_napi_err)
  }

  /// Replace text within a file. Calls the `edit_file` tool directly.
  ///
  /// Finds the exact string `oldString` in the file at `path` and replaces it
  /// with `newString`. By default `oldString` must appear exactly once —
  /// include enough surrounding context to make it unique. Set `replaceAll` to
  /// `true` to replace every occurrence instead.
  /// Always read the file before editing to ensure `oldString` matches the
  /// current content exactly.
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
