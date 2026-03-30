use schemars::JsonSchema;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{Result, Sandbox, SpadeboxError, sandbox::map_io_err};

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

// ---------------------------------------------------------------------------
// read_file
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadParams {
    /// Path to the file to read, relative to the sandbox root.
    pub path: String,
}

pub struct ReadFileTool;

impl Tool for ReadFileTool {
    type Params = ReadParams;
    const NAME: &'static str = "read_file";
    const DESCRIPTION: &'static str =
        "Read the full text content of a file. \
         Provide a relative path (e.g. 'src/main.rs' or 'README.md'). \
         Returns the file's content as a UTF-8 string.";

    async fn run(sandbox: &Sandbox, params: ReadParams) -> Result<String> {
        let file = sandbox.root.open(&params.path).map_err(map_io_err)?;
        let mut tokio_file = tokio::fs::File::from_std(file.into_std());
        let mut buf = Vec::new();
        tokio_file.read_to_end(&mut buf).await.map_err(SpadeboxError::IoError)?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }
}

// ---------------------------------------------------------------------------
// write_file
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteParams {
    /// Path to the file to write, relative to the sandbox root.
    pub path: String,
    /// Content to write (UTF-8).
    pub content: String,
}

pub struct WriteFileTool;

impl Tool for WriteFileTool {
    type Params = WriteParams;
    const NAME: &'static str = "write_file";
    const DESCRIPTION: &'static str =
        "Write text content to a file. \
         Provide a relative path (e.g. 'output.txt') and the full UTF-8 content to write. \
         Creates the file if it does not exist, or overwrites it entirely if it does.";

    async fn run(sandbox: &Sandbox, params: WriteParams) -> Result<String> {
        let file = sandbox.root.create(&params.path).map_err(map_io_err)?;
        let mut tokio_file = tokio::fs::File::from_std(file.into_std());
        tokio_file.write_all(params.content.as_bytes()).await.map_err(SpadeboxError::IoError)?;
        Ok(format!("Wrote {} bytes to {}", params.content.len(), params.path))
    }
}
