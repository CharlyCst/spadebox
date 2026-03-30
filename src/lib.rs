mod error;
mod sandbox;

pub use error::SpadeboxError;
pub use sandbox::Sandbox;

pub type Result<T> = std::result::Result<T, SpadeboxError>;
