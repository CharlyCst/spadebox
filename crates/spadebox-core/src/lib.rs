mod error;
pub mod grep;
mod sandbox;
pub mod tools;

pub use error::SpadeboxError;
pub use sandbox::Sandbox;
pub use tools::Tool;

pub type Result<T> = std::result::Result<T, SpadeboxError>;
