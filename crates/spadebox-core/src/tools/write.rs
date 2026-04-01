use schemars::JsonSchema;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use crate::{sandbox::map_io_err, Result, Sandbox, SpadeboxError};

use super::Tool;

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
    const DESCRIPTION: &'static str = "Write text content to a file. \
         Provide a relative path (e.g. 'output.txt') and the full UTF-8 content to write. \
         Creates the file if it does not exist, or overwrites it entirely if it does.";

    async fn run(sandbox: &Sandbox, params: WriteParams) -> Result<String> {
        let file = sandbox
            .root
            .create(&params.path)
            .map_err(|e| map_io_err(&params.path, e))?;
        let mut tokio_file = tokio::fs::File::from_std(file.into_std());
        tokio_file
            .write_all(params.content.as_bytes())
            .await
            .map_err(SpadeboxError::IoError)?;
        Ok(format!(
            "Wrote {} bytes to {}",
            params.content.len(),
            params.path
        ))
    }
}
