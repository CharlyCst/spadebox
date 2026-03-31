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

// ---------------------------------------------------------------------------
// edit_file
// ---------------------------------------------------------------------------

/// Accepts both JSON booleans (`true`) and strings (`"true"`/`"false"`).
/// MCP clients such as Claude Code may serialize booleans as strings.
fn deserialize_bool_flexible<'de, D: serde::Deserializer<'de>>(
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditParams {
    /// Path to the file to edit, relative to the sandbox root.
    pub path: String,
    /// Exact string to search for. Must appear exactly once unless replace_all is true.
    /// Include enough surrounding context to uniquely identify the location.
    pub old_string: String,
    /// String to replace it with.
    pub new_string: String,
    /// If true, replace every occurrence instead of requiring exactly one. Defaults to false.
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub replace_all: bool,
}

pub struct EditFileTool;

impl Tool for EditFileTool {
    type Params = EditParams;
    const NAME: &'static str = "edit_file";
    const DESCRIPTION: &'static str = "Replace text within a file. Reads the file, finds the exact string provided in 'old_string', \
         and replaces it with 'new_string'. By default the string must appear exactly once — include \
         enough surrounding context in 'old_string' to make it unique. \
         If the string appears multiple times and you want to replace all of them, set replace_all to true. \
         Always read the file before editing to ensure 'old_string' matches the current content exactly.";

    async fn run(sandbox: &Sandbox, params: EditParams) -> Result<String> {
        // Read
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
        let content =
            String::from_utf8(buf).map_err(|_| SpadeboxError::NotUtf8(params.path.clone()))?;

        // Validate
        let count = content.matches(params.old_string.as_str()).count();
        match count {
            0 => return Err(SpadeboxError::StringNotFound(params.path.clone())),
            n if n > 1 && !params.replace_all => {
                return Err(SpadeboxError::AmbiguousEdit {
                    path: params.path.clone(),
                    count: n,
                });
            }
            _ => {}
        }

        // Replace and write back
        let updated = if params.replace_all {
            content.replace(params.old_string.as_str(), &params.new_string)
        } else {
            content.replacen(params.old_string.as_str(), &params.new_string, 1)
        };
        let file = sandbox
            .root
            .create(&params.path)
            .map_err(|e| map_io_err(&params.path, e))?;
        let mut tokio_file = tokio::fs::File::from_std(file.into_std());
        tokio_file
            .write_all(updated.as_bytes())
            .await
            .map_err(SpadeboxError::IoError)?;

        let replacements = if params.replace_all { count } else { 1 };
        Ok(format!(
            "Replaced {} occurrence(s) in '{}'",
            replacements, params.path
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::EditParams;

    #[test]
    fn deserialize_bool_flexible() {
        fn parse(replace_all: &str) -> EditParams {
            serde_json::from_str(&format!(
                r#"{{"path":"f","old_string":"a","new_string":"b","replace_all":{replace_all}}}"#
            ))
            .unwrap()
        }

        // JSON booleans
        assert!(parse("true").replace_all);
        assert!(!parse("false").replace_all);

        // String booleans (sent by some MCP clients)
        assert!(parse(r#""true""#).replace_all);
        assert!(!parse(r#""false""#).replace_all);

        // Absent field defaults to false
        let p: EditParams = serde_json::from_str(r#"{"path":"f","old_string":"a","new_string":"b"}"#).unwrap();
        assert!(!p.replace_all);

        // Invalid string is rejected
        let result: serde_json::Result<EditParams> =
            serde_json::from_str(r#"{"path":"f","old_string":"a","new_string":"b","replace_all":"yes"}"#);
        assert!(result.is_err());
    }
}
