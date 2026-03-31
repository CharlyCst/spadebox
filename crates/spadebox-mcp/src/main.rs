use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::tool::schema_for_type,
    model::{
        CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool as McpToolDef,
    },
    service::RequestContext,
};
use spadebox_core::{
    Sandbox, Tool,
    grep::GrepTool,
    tools::{EditFileTool, ReadFileTool, WriteFileTool},
};

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

/// Build an `rmcp` tool descriptor from a `Tool` implementation.
fn mcp_tool<T: Tool>() -> McpToolDef
where
    T::Params: 'static,
{
    McpToolDef::new(T::NAME, T::DESCRIPTION, schema_for_type::<T::Params>())
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
            tools: vec![
                mcp_tool::<ReadFileTool>(),
                mcp_tool::<WriteFileTool>(),
                mcp_tool::<EditFileTool>(),
                mcp_tool::<GrepTool>(),
            ],
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::Value::Object(request.arguments.unwrap_or_default());

        let result = match request.name.as_ref() {
            ReadFileTool::NAME => {
                let params = serde_json::from_value(args)
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
                ReadFileTool::run(&self.sandbox, params).await
            }
            WriteFileTool::NAME => {
                let params = serde_json::from_value(args)
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
                WriteFileTool::run(&self.sandbox, params).await
            }
            EditFileTool::NAME => {
                let params = serde_json::from_value(args)
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
                EditFileTool::run(&self.sandbox, params).await
            }
            GrepTool::NAME => {
                let params = serde_json::from_value(args)
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
                GrepTool::run(&self.sandbox, params).await
            }
            name => {
                return Err(McpError::invalid_params(
                    format!("unknown tool: {name}"),
                    None,
                ));
            }
        };

        match result {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let sandbox_root = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());

    let sandbox = Sandbox::new(&sandbox_root)
        .map_err(|e| anyhow::anyhow!("Failed to open sandbox at {sandbox_root:?}: {e}"))?;

    let service = SpadeboxMcpServer::new(sandbox)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
