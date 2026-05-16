// Doc comments in this file are surfaced as Python API documentation via
// PyO3. Parameter names use snake_case, matching Python conventions.

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use spadebox_core::{
    DomainRule, HttpVerb, Sandbox, enabled_tools,
    tools::{
        EditFileTool, EditParams, FetchParams, FetchTool, GlobParams, GlobTool, GrepParams,
        GrepTool, JsExecParams, JsExecTool, JsReplParams, JsReplTool, MoveParams, MoveTool,
        ReadFileTool, ReadParams, Tool, WriteFileTool, WriteParams,
    },
};
use std::sync::Arc;
use tokio::runtime::Runtime;

fn to_py_err(e: spadebox_core::ToolError) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

/// Tool metadata exposed to Python.
#[pyclass]
pub struct SbTool {
    /// The tool's name, used when calling `call_tool`.
    #[pyo3(get)]
    pub name: String,
    /// Human-readable description of what the tool does.
    #[pyo3(get)]
    pub description: String,
    /// JSON Schema for the tool's parameters, serialized as a JSON string.
    #[pyo3(get)]
    pub input_schema: String,
}

#[pymethods]
impl SbTool {
    fn __repr__(&self) -> String {
        format!("SbTool(name={:?})", self.name)
    }
}

/// Result of a tool call.
///
/// Distinct from a Python exception: an `SbToolResult` is always returned
/// on successful dispatch. Use `is_error` to tell the agent whether the tool
/// succeeded or encountered a domain error (e.g. file not found).
/// A Python exception is only raised for protocol errors (unknown tool name,
/// malformed params JSON) that indicate a developer mistake.
#[pyclass]
pub struct SbToolResult {
    /// `True` when the tool encountered a domain error intended for the agent.
    #[pyo3(get)]
    pub is_error: bool,
    /// The tool's output (success) or error message (tool-level error).
    #[pyo3(get)]
    pub output: String,
}

#[pymethods]
impl SbToolResult {
    fn __repr__(&self) -> String {
        format!(
            "SbToolResult(is_error={}, output={:?})",
            self.is_error, self.output
        )
    }
}

/// SpadeBox provides sandboxed tools for AI agents.
///
/// All tools are disabled by default. Call `enable_files`, `enable_http`,
/// or `enable_js` to activate them. Configuration methods return `self`
/// so they can be chained::
///
///     sb = SpadeBox().enable_files("/path/to/sandbox")
///     content = sb.read_file("README.md")
#[pyclass]
pub struct SpadeBox {
    inner: Arc<Sandbox>,
    runtime: Arc<Runtime>,
}

#[pymethods]
impl SpadeBox {
    /// Create a new SpadeBox instance with all tools disabled.
    ///
    /// Call `enable_files` to enable filesystem tools and `enable_http` to
    /// enable HTTP fetching.
    #[new]
    pub fn new() -> PyResult<Self> {
        let runtime = Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("failed to create tokio runtime: {e}")))?;
        Ok(Self {
            inner: Arc::new(Sandbox::new()),
            runtime: Arc::new(runtime),
        })
    }

    /// Enable filesystem tools with `path` as the sandbox root.
    ///
    /// All file-system operations are restricted to this directory.
    /// Raises `RuntimeError` if `path` cannot be opened.
    /// Returns `self` for method chaining.
    pub fn enable_files(slf: Py<Self>, py: Python<'_>, path: String) -> PyResult<Py<Self>> {
        slf.borrow(py).inner.enable_fs(&path).map_err(to_py_err)?;
        Ok(slf)
    }

    /// Enable HTTP fetching.
    ///
    /// After enabling, use `allow` to add domain rules controlling which
    /// hosts and HTTP verbs are permitted.
    /// Returns `self` for method chaining.
    pub fn enable_http(slf: Py<Self>, py: Python<'_>) -> Py<Self> {
        slf.borrow(py).inner.enable_http();
        slf
    }

    /// Enable the JavaScript tools.
    ///
    /// Once enabled, the JS REPL session persists across calls: variables and
    /// functions defined in one `js_repl` call are available in subsequent ones.
    /// Returns `self` for method chaining.
    pub fn enable_js(slf: Py<Self>, py: Python<'_>) -> Py<Self> {
        slf.borrow(py).inner.enable_js();
        slf
    }

    /// Set the `User-Agent` header sent with every HTTP request.
    ///
    /// Defaults to `"spadebox/0.0.0 (AI-agent)"`.
    /// Returns `self` for method chaining.
    pub fn set_user_agent(slf: Py<Self>, py: Python<'_>, user_agent: String) -> Py<Self> {
        slf.borrow(py).inner.set_user_agent(user_agent);
        slf
    }

    /// Add a domain rule permitting the given HTTP verbs for `pattern`.
    ///
    /// `pattern` may be an exact hostname (``"api.example.com"``), a wildcard
    /// subdomain (``"*.example.com"``), or a catch-all (``"*"``). When multiple
    /// rules match a request, the most specific one wins (longest literal suffix).
    ///
    /// `verbs` is a list of HTTP method strings (e.g. ``["GET", "POST"]``).
    ///
    /// Raises `ValueError` if `pattern` is invalid or any verb is unrecognised.
    /// Returns `self` for method chaining.
    pub fn allow(
        slf: Py<Self>,
        py: Python<'_>,
        pattern: String,
        verbs: Vec<String>,
    ) -> PyResult<Py<Self>> {
        let parsed_verbs = verbs
            .iter()
            .map(|v| {
                HttpVerb::parse(v.to_uppercase().as_str())
                    .ok_or_else(|| PyValueError::new_err(format!("unknown HTTP verb '{v}'")))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let rule = DomainRule::new(pattern, parsed_verbs).map_err(to_py_err)?;
        slf.borrow(py).inner.allow(rule);
        Ok(slf)
    }

    /// Return metadata for all currently enabled tools.
    ///
    /// Returns a list of `SbTool` objects, each with `name`, `description`,
    /// and `input_schema` (a JSON string).
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
    /// Raises a Python exception on protocol errors (unknown tool name or
    /// malformed `params_json`). Returns an `SbToolResult` in all other cases —
    /// check `is_error` to distinguish tool success from tool-level errors.
    pub fn call_tool(
        &self,
        py: Python<'_>,
        name: String,
        params_json: String,
    ) -> PyResult<SbToolResult> {
        let inner = Arc::clone(&self.inner);
        let runtime = Arc::clone(&self.runtime);
        py.detach(|| {
            runtime.block_on(async move {
                match spadebox_core::call_tool(&inner, &name, params_json).await {
                    Err(protocol_err) => Err(PyValueError::new_err(protocol_err)),
                    Ok(Ok(output)) => Ok(SbToolResult {
                        is_error: false,
                        output,
                    }),
                    Ok(Err(tool_err)) => Ok(SbToolResult {
                        is_error: true,
                        output: tool_err.to_string(),
                    }),
                }
            })
        })
    }

    /// Read the text content of a file.
    ///
    /// `path` must be relative to the sandbox root (e.g. ``"src/main.rs"``).
    /// `offset` is a 1-indexed line number to start reading from (default: 1).
    /// `limit` caps the number of lines returned.
    /// `max_bytes` caps the number of bytes returned (default: 20 000). Pass ``0`` to disable.
    ///
    /// Returns the file's content as a UTF-8 string.
    #[pyo3(signature = (path, offset=None, limit=None, max_bytes=None))]
    pub fn read_file(
        &self,
        py: Python<'_>,
        path: String,
        offset: Option<u64>,
        limit: Option<u64>,
        max_bytes: Option<u64>,
    ) -> PyResult<String> {
        let inner = Arc::clone(&self.inner);
        let runtime = Arc::clone(&self.runtime);
        py.detach(|| {
            runtime.block_on(async move {
                ReadFileTool::run(
                    &inner,
                    ReadParams {
                        path,
                        offset,
                        limit,
                        max_bytes,
                    },
                )
                .await
                .map_err(to_py_err)
            })
        })
    }

    /// Write text content to a file.
    ///
    /// `path` must be relative to the sandbox root (e.g. ``"src/main.rs"``).
    /// Creates the file if it does not exist, or overwrites it entirely if it does.
    /// If the file already exists, it must be read first — attempting to overwrite
    /// without a prior `read_file` call will raise an error.
    /// Set `create_dirs` to `True` to create any missing intermediate directories
    /// automatically. To create a directory without writing a file, end `path`
    /// with ``"/"`` (e.g. ``"src/utils/"``).
    #[pyo3(signature = (path, content=None, create_dirs=None))]
    pub fn write_file(
        &self,
        py: Python<'_>,
        path: String,
        content: Option<String>,
        create_dirs: Option<bool>,
    ) -> PyResult<String> {
        let inner = Arc::clone(&self.inner);
        let runtime = Arc::clone(&self.runtime);
        py.detach(|| {
            runtime.block_on(async move {
                WriteFileTool::run(
                    &inner,
                    WriteParams {
                        path,
                        content: content.unwrap_or_default(),
                        create_dirs: create_dirs.unwrap_or(false),
                    },
                )
                .await
                .map_err(to_py_err)
            })
        })
    }

    /// Replace text within a file.
    ///
    /// Finds the exact string `old_string` in the file at `path` and replaces
    /// it with `new_string`. By default `old_string` must appear exactly once —
    /// include enough surrounding context to make it unique. Set `replace_all`
    /// to `True` to replace every occurrence instead.
    ///
    /// Always read the file before editing to ensure `old_string` matches the
    /// current content exactly.
    #[pyo3(signature = (path, old_string, new_string, replace_all=None))]
    pub fn edit_file(
        &self,
        py: Python<'_>,
        path: String,
        old_string: String,
        new_string: String,
        replace_all: Option<bool>,
    ) -> PyResult<String> {
        let inner = Arc::clone(&self.inner);
        let runtime = Arc::clone(&self.runtime);
        py.detach(|| {
            runtime.block_on(async move {
                EditFileTool::run(
                    &inner,
                    EditParams {
                        path,
                        old_string,
                        new_string,
                        replace_all: replace_all.unwrap_or(false),
                    },
                )
                .await
                .map_err(to_py_err)
            })
        })
    }

    /// Move or rename a file or directory, or delete it.
    ///
    /// `src` is the source path relative to the sandbox root. `dst` is the
    /// destination path; omit it (pass `None`) when deleting.
    /// Set `overwrite` to `True` to replace an existing destination.
    /// Set `delete` to `True` (with no `dst`) to delete `src` instead of
    /// moving it.
    /// Set `create_dirs` to `True` to create any missing intermediate
    /// directories for the destination.
    #[pyo3(signature = (src, dst=None, overwrite=None, delete=None, create_dirs=None))]
    pub fn move_path(
        &self,
        py: Python<'_>,
        src: String,
        dst: Option<String>,
        overwrite: Option<bool>,
        delete: Option<bool>,
        create_dirs: Option<bool>,
    ) -> PyResult<String> {
        let inner = Arc::clone(&self.inner);
        let runtime = Arc::clone(&self.runtime);
        py.detach(|| {
            runtime.block_on(async move {
                MoveTool::run(
                    &inner,
                    MoveParams {
                        src,
                        dst,
                        overwrite: overwrite.unwrap_or(false),
                        delete: delete.unwrap_or(false),
                        create_dirs: create_dirs.unwrap_or(false),
                    },
                )
                .await
                .map_err(to_py_err)
            })
        })
    }

    /// List files matching a glob pattern.
    ///
    /// Returns a newline-separated list of relative file paths sorted
    /// alphabetically. Use ``"**"`` to match across directories
    /// (e.g. ``"**/*.rs"`` finds all Rust files).
    pub fn glob(&self, py: Python<'_>, pattern: String) -> PyResult<String> {
        let inner = Arc::clone(&self.inner);
        let runtime = Arc::clone(&self.runtime);
        py.detach(|| {
            runtime.block_on(async move {
                GlobTool::run(&inner, GlobParams { pattern })
                    .await
                    .map_err(to_py_err)
            })
        })
    }

    /// Search file contents for a regex pattern.
    ///
    /// Returns matching lines with their file path and line number.
    /// Use `glob` to restrict the search to specific file types
    /// (e.g. ``"**/*.rs"``). Use `context_lines` to include N surrounding
    /// lines around each match.
    #[pyo3(signature = (pattern, glob=None, context_lines=None))]
    pub fn grep(
        &self,
        py: Python<'_>,
        pattern: String,
        glob: Option<String>,
        context_lines: Option<u32>,
    ) -> PyResult<String> {
        let inner = Arc::clone(&self.inner);
        let runtime = Arc::clone(&self.runtime);
        py.detach(|| {
            runtime.block_on(async move {
                GrepTool::run(
                    &inner,
                    GrepParams {
                        pattern,
                        glob,
                        context_lines: context_lines.unwrap_or(0),
                    },
                )
                .await
                .map_err(to_py_err)
            })
        })
    }

    /// Perform an HTTP request and return the response body as text.
    ///
    /// HTTP must be enabled first via `enable_http`. The `url` must use the
    /// ``http`` or ``https`` scheme. `method` is case-insensitive (e.g.
    /// ``"GET"``, ``"POST"``).
    /// Pass `body` for methods that send a request body (POST, PUT, PATCH).
    /// When `raw` is `False` (default), HTML responses are converted to
    /// Markdown. Set `raw` to `True` to receive the raw response body.
    /// `max_bytes` caps the number of bytes returned (default: 20 000).
    /// Pass ``0`` to disable.
    #[pyo3(signature = (url, method, body=None, raw=None, max_bytes=None))]
    pub fn fetch(
        &self,
        py: Python<'_>,
        url: String,
        method: String,
        body: Option<String>,
        raw: Option<bool>,
        max_bytes: Option<u64>,
    ) -> PyResult<String> {
        let inner = Arc::clone(&self.inner);
        let runtime = Arc::clone(&self.runtime);
        py.detach(|| {
            runtime.block_on(async move {
                FetchTool::run(
                    &inner,
                    FetchParams {
                        url,
                        method,
                        body,
                        raw: raw.unwrap_or(false),
                        max_bytes,
                    },
                )
                .await
                .map_err(to_py_err)
            })
        })
    }

    /// Expose a Python callable as a JavaScript global function.
    ///
    /// `name` is the JavaScript identifier the function will be available as.
    /// `params` declares the parameter names in order. JS positional arguments
    /// are mapped to a Python dict ``{"paramName": value, ...}`` and passed to
    /// `func`. `func` must return a JSON-serialisable value. The function is
    /// available in all subsequent `js_repl` calls and in every `js_exec`
    /// context::
    ///
    ///     sb = SpadeBox().enable_js()
    ///     sb.expose_js_func("add", ["a", "b"], lambda args: args["a"] + args["b"])
    ///     result = sb.js_repl("add(1, 2)")  # "3"
    pub fn expose_js_func(
        &self,
        name: String,
        params: Vec<String>,
        func: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let func = func.unbind();
        let inner = Arc::clone(&self.inner);
        inner.expose_js_func(
            name,
            params,
            move |args: serde_json::Value| -> Result<serde_json::Value, String> {
                Python::attach(|py| {
                    // Deserialise args JSON into a Python dict via json.loads.
                    let json_str = serde_json::to_string(&args).map_err(|e| e.to_string())?;
                    let json_mod = py.import("json").map_err(|e: PyErr| e.to_string())?;
                    let py_args = json_mod
                        .call_method1("loads", (json_str,))
                        .map_err(|e: PyErr| e.to_string())?;

                    // Call the user's Python function with the dict.
                    let tuple = PyTuple::new(py, [py_args]).map_err(|e: PyErr| e.to_string())?;
                    let result = func.call1(py, tuple).map_err(|e: PyErr| e.to_string())?;

                    // Serialise the return value back to JSON via json.dumps.
                    let result_str: String = json_mod
                        .call_method1("dumps", (result.bind(py),))
                        .map_err(|e: PyErr| e.to_string())?
                        .extract()
                        .map_err(|e: PyErr| e.to_string())?;

                    serde_json::from_str(&result_str).map_err(|e| e.to_string())
                })
            },
        )
        .map_err(to_py_err)
    }

    /// Evaluate JavaScript code and return the result as a string.
    ///
    /// The session is persistent: variables and functions defined in one call
    /// are available in subsequent calls. Requires `enable_js` to have been
    /// called first.
    pub fn js_repl(&self, py: Python<'_>, code: String) -> PyResult<String> {
        let inner = Arc::clone(&self.inner);
        let runtime = Arc::clone(&self.runtime);
        py.detach(|| {
            runtime.block_on(async move {
                JsReplTool::run(&inner, JsReplParams { code })
                    .await
                    .map_err(to_py_err)
            })
        })
    }

    /// Execute a JavaScript file in a fresh runtime.
    ///
    /// `path` must be relative to the sandbox root (e.g. ``"scripts/setup.js"``).
    /// Each call starts from a clean context — no state is shared with the JS
    /// REPL or other `js_exec` calls. Requires both `enable_js` and
    /// `enable_files`. Returns an empty string on success, or raises if the
    /// script throws.
    pub fn js_exec(&self, py: Python<'_>, path: String) -> PyResult<String> {
        let inner = Arc::clone(&self.inner);
        let runtime = Arc::clone(&self.runtime);
        py.detach(|| {
            runtime.block_on(async move {
                JsExecTool::run(&inner, JsExecParams { path })
                    .await
                    .map_err(to_py_err)
            })
        })
    }
}

#[pymodule]
fn spadebox(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SpadeBox>()?;
    m.add_class::<SbTool>()?;
    m.add_class::<SbToolResult>()?;
    Ok(())
}
