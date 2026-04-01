use schemars::JsonSchema;
use serde::Deserialize;
use tokio::io::AsyncReadExt;

use crate::{sandbox::map_io_err, Result, Sandbox, SpadeboxError};

use super::Tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadParams {
    /// Path to the file to read, relative to the sandbox root.
    pub path: String,
}

pub struct ReadFileTool;

impl Tool for ReadFileTool {
    type Params = ReadParams;
    const NAME: &'static str = "read_file";
    const DESCRIPTION: &'static str = "Read the full text content of a file. \
         Provide a relative path (e.g. 'src/main.rs' or 'README.md'). \
         Returns the file's content as a UTF-8 string.";

    async fn run(sandbox: &Sandbox, params: ReadParams) -> Result<String> {
        let file = sandbox
            .root
            .open(&params.path)
            .map_err(|e| map_io_err(&params.path, e))?;
        let mut tokio_file = tokio::fs::File::from_std(file.into_std());
        let mut buf = Vec::new();
        tokio_file
            .read_to_end(&mut buf)
            .await
            .map_err(SpadeboxError::IoError)?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }
}
