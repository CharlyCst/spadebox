#[derive(Debug, thiserror::Error)]
pub enum SpadeboxError {
    #[error("path escapes sandbox: '{0}'")]
    EscapeAttempt(String),
    #[error("file not found: '{0}'")]
    NotFound(String),
    #[error("permission denied: '{0}'")]
    PermissionDenied(String),
    #[error("file is not valid UTF-8: '{0}'")]
    NotUtf8(String),
    #[error("string found {count} times in '{path}'; add more context to make it unique, or set replace_all to true")]
    AmbiguousEdit { path: String, count: usize },
    #[error("string not found in '{0}'")]
    StringNotFound(String),
    #[error("timeout")]
    Timeout,
    #[error("JS error: {0}")]
    JsError(String),
    #[error("I/O error: {0}")]
    IoError(std::io::Error),
}
