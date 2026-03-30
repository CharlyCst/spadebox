#[derive(Debug, thiserror::Error)]
pub enum SpadeboxError {
    #[error("path escapes sandbox: '{0}'")]
    EscapeAttempt(String),
    #[error("file not found: '{0}'")]
    NotFound(String),
    #[error("permission denied: '{0}'")]
    PermissionDenied(String),
    #[error("timeout")]
    Timeout,
    #[error("JS error: {0}")]
    JsError(String),
    #[error("I/O error: {0}")]
    IoError(std::io::Error),
}
