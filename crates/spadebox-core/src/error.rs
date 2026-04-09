/// The result type for tool execution — carries a [`ToolError`] on failure.
pub type ToolResult<T> = std::result::Result<T, ToolError>;

/// Errors produced during tool execution, intended to be surfaced to the AI agent.
///
/// `ToolError` represents **tool-level errors** — conditions the agent should
/// know about so it can adjust its next action (e.g. a file was not found, a path
/// escaped the sandbox, an edit string was ambiguous). These are distinct from
/// **protocol errors**, which indicate a developer mistake or API errors.
///
/// The canonical return type for tool execution is [`ToolResult<T>`], which is
/// `Result<T, ToolError>`. When exposing tools through a binding (JS, Python,
/// MCP, …) the `ToolError` message should be forwarded to the model as the
/// tool result rather than raised as an exception.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("path escapes sandbox: '{0}'")]
    EscapeAttempt(String),
    #[error("file not found: '{0}'")]
    NotFound(String),
    #[error("permission denied: '{0}'")]
    PermissionDenied(String),
    #[error("file is not valid UTF-8: '{0}'")]
    NotUtf8(String),
    #[error(
        "string found {count} times in '{path}'; add more context to make it unique, or set replace_all to true"
    )]
    AmbiguousEdit { path: String, count: usize },
    #[error("string not found in '{0}'")]
    StringNotFound(String),
    #[error("file '{0}' must be read before it can be written")]
    NotRead(String),
    #[error("file '{0}' was modified externally since it was last read, it must be read again before it can be written")]
    FileModified(String),
    #[error("invalid pattern: {0}")]
    InvalidPattern(String),
    #[error("timeout")]
    Timeout,
    #[error("JS error: {0}")]
    JsError(String),
    #[error("I/O error: {0}")]
    IoError(std::io::Error),
}
