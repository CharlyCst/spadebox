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
            tools: enabled_tools(&self.sandbox).into_iter().map(to_mcp_tool).collect(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_string(
            &serde_json::Value::Object(request.arguments.unwrap_or_default()),
        )
        .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

        match spadebox_core::call_tool(&self.sandbox, &request.name, args).await {
            Err(protocol_err) => Err(McpError::invalid_params(protocol_err, None)),
            Ok(Ok(text)) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Ok(Err(e)) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let sandbox_root = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());

    let mut sandbox = Sandbox::new();
    sandbox
        .files
        .enable(&sandbox_root)
        .map_err(|e| anyhow::anyhow!("Failed to open sandbox at {sandbox_root:?}: {e}"))?;

    let service = SpadeboxMcpServer::new(sandbox)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
