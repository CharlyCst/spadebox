use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool as McpToolDef,
    },
    service::RequestContext,
};
use spadebox_core::{Sandbox, ToolDef, enabled_tools};

#[derive(Clone)]
struct SpadeboxMcpServer {
    sandbox: Arc<Sandbox>,
}

impl SpadeboxMcpServer {
    fn new(sandbox: Sandbox) -> Self {
        Self {
            sandbox: Arc::new(sandbox),
        }
    }
}

fn to_mcp_tool(def: ToolDef) -> McpToolDef {
    let schema = match def.schema {
        serde_json::Value::Object(map) => Arc::new(map),
        _ => Arc::new(serde_json::Map::new()),
    };
    McpToolDef::new(def.name, def.description, schema)
}

impl ServerHandler for SpadeboxMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: enabled_tools(&self.sandbox)
                .into_iter()
                .map(to_mcp_tool)
                .collect(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_string(&serde_json::Value::Object(
            request.arguments.unwrap_or_default(),
        ))
        .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

        match spadebox_core::call_tool(&self.sandbox, &request.name, args).await {
            Err(protocol_err) => Err(McpError::invalid_params(protocol_err, None)),
            Ok(Ok(text)) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Ok(Err(e)) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

/// Spadebox MCP server.
///
/// All tools are disabled by default. Use `--files` to enable filesystem tools
/// and `--allow` to enable HTTP fetching for specific domains.
#[derive(clap::Parser)]
#[command(version, about)]
struct Cli {
    /// Enable filesystem tools with PATH as the sandbox root.
    #[arg(long)]
    files: Option<String>,

    /// Enable HTTP and add a domain rule. Format: `<pattern>:<verbs>` where
    /// verbs is a comma-separated list (e.g. `api.example.com:GET,POST` or
    /// `*:GET`). May be repeated.
    #[arg(long)]
    allow: Vec<String>,

    /// Enable the JavaScript tools.
    #[arg(long)]
    js: bool,
}

fn parse_allow(rule: &str) -> anyhow::Result<(String, Vec<String>)> {
    let (pattern, verbs_str) = rule
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("--allow {rule:?}: expected format <pattern>:<verbs>"))?;
    let verbs = verbs_str
        .split(',')
        .map(|v| v.trim().to_uppercase())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();
    if verbs.is_empty() {
        anyhow::bail!("--allow {rule:?}: no verbs specified");
    }
    Ok((pattern.to_string(), verbs))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use clap::{CommandFactory, Parser};

    let cli = Cli::parse();

    if cli.files.is_none() && cli.allow.is_empty() && !cli.js {
        Cli::command()
            .error(
                clap::error::ErrorKind::MissingRequiredArgument,
                "at least one of --files, --allow, or --js must be specified",
            )
            .exit();
    }

    let mut sandbox = Sandbox::new();

    if let Some(path) = &cli.files {
        sandbox
            .files
            .enable(path)
            .map_err(|e| anyhow::anyhow!("--files {path:?}: {e}"))?;
    }

    if cli.js {
        sandbox.js.enable();
    }

    if !cli.allow.is_empty() {
        sandbox.http.enable();
        for rule in &cli.allow {
            let (pattern, verbs) = parse_allow(rule)?;
            let domain_rule = spadebox_core::DomainRule::new(
                pattern,
                verbs
                    .iter()
                    .map(|v| {
                        spadebox_core::HttpVerb::parse(v)
                            .ok_or_else(|| anyhow::anyhow!("--allow {rule:?}: unknown verb {v:?}"))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?,
            )
            .map_err(|e| anyhow::anyhow!("--allow {rule:?}: {e}"))?;
            sandbox.http.allow(domain_rule);
        }
    }

    let service = SpadeboxMcpServer::new(sandbox)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_allow_cases() {
        // exact hostname, single verb
        let (pattern, verbs) = parse_allow("api.example.com:GET").unwrap();
        assert_eq!(pattern, "api.example.com");
        assert_eq!(verbs, ["GET"]);

        // exact hostname, multiple verbs
        let (pattern, verbs) = parse_allow("api.example.com:GET,POST").unwrap();
        assert_eq!(pattern, "api.example.com");
        assert_eq!(verbs, ["GET", "POST"]);

        // catch-all wildcard
        let (pattern, verbs) = parse_allow("*:GET").unwrap();
        assert_eq!(pattern, "*");
        assert_eq!(verbs, ["GET"]);

        // subdomain wildcard
        let (pattern, verbs) = parse_allow("*.example.com:GET,DELETE").unwrap();
        assert_eq!(pattern, "*.example.com");
        assert_eq!(verbs, ["GET", "DELETE"]);

        // verbs are uppercased
        let (_, verbs) = parse_allow("example.com:get,post").unwrap();
        assert_eq!(verbs, ["GET", "POST"]);

        // whitespace around verbs is trimmed
        let (_, verbs) = parse_allow("example.com:GET, POST , DELETE").unwrap();
        assert_eq!(verbs, ["GET", "POST", "DELETE"]);

        // missing colon
        assert!(parse_allow("example.com").is_err());

        // empty verbs
        assert!(parse_allow("example.com:").is_err());
    }
}
