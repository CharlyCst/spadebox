mod error;
mod sandbox;
pub mod tools;

pub use error::SpadeboxError;
pub use sandbox::Sandbox;
pub use tools::{Tool, call_tool};

/// The result type for tool execution — carries a [`SpadeboxError`] on failure.
pub type ToolResult<T> = std::result::Result<T, SpadeboxError>;
