use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Result, Sandbox};

mod edit;
mod glob;
mod grep;
mod read;
mod write;

pub use edit::{EditFileTool, EditParams};
pub use glob::{GlobParams, GlobTool};
pub use grep::{GrepParams, GrepTool};
pub use read::{ReadFileTool, ReadParams};
pub use write::{WriteFileTool, WriteParams};

/// A sandboxed tool that can be exposed through any interface (MCP, Python, JS, …).
///
/// Implementors define their own strongly-typed `Params`, carry their own `NAME`
/// and `DESCRIPTION`, and provide the async `run` logic. Interface crates
/// (spadebox-mcp, etc.) use these constants and call `run` — they add no logic of
/// their own.
pub trait Tool {
    /// Strongly-typed parameters, deserializable from JSON and self-describing via
    /// JSON Schema so every interface can expose an accurate schema without
    /// duplicating the type definition.
    type Params: for<'de> Deserialize<'de> + JsonSchema;

    /// Canonical tool name exposed to callers (e.g. `"read_file"`).
    const NAME: &'static str;

    /// Human-readable description of what the tool does.
    const DESCRIPTION: &'static str;

    /// Execute the tool against `sandbox` with the given `params`.
    /// Returns a plain UTF-8 string result suitable for wrapping in any
    /// interface's response type.
    fn run(
        sandbox: &Sandbox,
        params: Self::Params,
    ) -> impl std::future::Future<Output = Result<String>> + Send;
}

/// Deserializes a boolean that may arrive as a JSON bool or as a string.
/// MCP clients such as Claude Code may serialize booleans as strings (`"true"`).
pub(super) fn deserialize_bool_flexible<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrString {
        Bool(bool),
        Str(String),
    }
    match BoolOrString::deserialize(d)? {
        BoolOrString::Bool(b) => Ok(b),
        BoolOrString::Str(s) => s.parse().map_err(serde::de::Error::custom),
    }
}
