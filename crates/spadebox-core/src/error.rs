#[derive(Debug, thiserror::Error)]
pub enum SpadeboxError {
    #[error("path escape attempt")]
    EscapeAttempt,
    #[error("not found")]
    NotFound,
    #[error("permission denied")]
    PermissionDenied,
    #[error("timeout")]
    Timeout,
    #[error("JS error: {0}")]
    JsError(String),
    #[error("I/O error: {0}")]
    IoError(std::io::Error),
}
