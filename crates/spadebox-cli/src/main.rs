use std::env;
use std::sync::Arc;

use anyhow::Context;
use spadebox_core::{DomainRule, HttpVerb, Sandbox, ToolDef, enabled_tools};

/// Spadebox CLI — run spadebox tools from the command line.
///
/// The sandbox is configured to allow all tools: filesystem tools are rooted at
/// the current working directory, HTTP is allowed to any host, and JS is enabled.
#[derive(clap::Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Manage tools
    Tools {
        #[command(subcommand)]
        action: ToolsAction,
    },
    /// Run a tool with JSON arguments
    Run {
        /// Name of the tool to run
        tool_name: String,
        /// Tool arguments as a JSON object (e.g. '{"path": "src/main.rs"}')
        json_args: String,
    },
}

#[derive(clap::Subcommand)]
enum ToolsAction {
    /// List all available tools
    List,
    /// Show detailed information about a tool
    Info {
        /// Name of the tool
        tool_name: String,
        /// Output in Markdown format
        #[arg(long)]
        markdown: bool,
    },
}

fn build_sandbox() -> anyhow::Result<Arc<Sandbox>> {
    let cwd = env::current_dir().context("failed to get current working directory")?;
    let sandbox = Sandbox::new();

    sandbox
        .enable_fs(&cwd)
        .with_context(|| format!("failed to open sandbox root at {}", cwd.display()))?;

    sandbox.enable_http().allow(
        DomainRule::new(
            "*",
            vec![
                HttpVerb::Get,
                HttpVerb::Post,
                HttpVerb::Put,
                HttpVerb::Patch,
                HttpVerb::Delete,
                HttpVerb::Head,
            ],
        )
        .expect("wildcard domain rule is always valid"),
    );

    sandbox.enable_js();

    Ok(Arc::new(sandbox))
}

/// Extracts the primary type name from a JSON Schema type value.
///
/// Handles both `"type": "string"` and `"type": ["integer", "null"]` (nullable
/// fields produced by schemars for `Option<T>`).
fn type_name(type_val: &serde_json::Value) -> &str {
    match type_val {
        serde_json::Value::String(s) => s.as_str(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .find_map(|v| {
                let s = v.as_str()?;
                (s != "null").then_some(s)
            })
            .unwrap_or("unknown"),
        _ => "unknown",
    }
}

fn print_tool_info(tool: &ToolDef, markdown: bool) {
    let schema = &tool.schema;
    let empty_map = serde_json::Map::new();
    let properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .unwrap_or(&empty_map);
    let required: std::collections::HashSet<&str> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let schema_str = serde_json::to_string_pretty(schema).unwrap_or_default();

    if markdown {
        println!("## Tool: {}", tool.name);
        println!();
        println!("{}", tool.description);
        println!();

        if !properties.is_empty() {
            println!("### Arguments");
            println!();
            for (name, prop) in properties {
                let ty = prop.get("type").map(type_name).unwrap_or("unknown");
                let req = if required.contains(name.as_str()) {
                    "required"
                } else {
                    "optional"
                };
                println!("**`{name}`** ({ty}, {req})");
                println!();
                if let Some(desc) = prop.get("description").and_then(|v| v.as_str()) {
                    println!("{desc}");
                    println!();
                }
            }
        }

        println!("### Schema");
        println!();
        println!("```json");
        println!("{schema_str}");
        println!("```");
    } else {
        println!("Tool: {}", tool.name);
        println!();
        println!("{}", tool.description);
        println!();

        if !properties.is_empty() {
            println!("Arguments:");
            for (name, prop) in properties {
                let ty = prop.get("type").map(type_name).unwrap_or("unknown");
                let req = if required.contains(name.as_str()) {
                    "required"
                } else {
                    "optional"
                };
                println!("  {name} ({ty}, {req})");
                if let Some(desc) = prop.get("description").and_then(|v| v.as_str()) {
                    for line in desc.lines() {
                        println!("    {line}");
                    }
                }
                println!();
            }
        }

        println!("Schema:");
        for line in schema_str.lines() {
            println!("  {line}");
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use clap::Parser;

    let cli = Cli::parse();
    let sandbox = build_sandbox()?;

    match cli.command {
        Command::Tools {
            action: ToolsAction::List,
        } => {
            let tools = enabled_tools(&sandbox);
            for tool in tools {
                println!("{}\n  {}\n", tool.name, tool.description);
            }
        }
        Command::Tools {
            action: ToolsAction::Info { tool_name, markdown },
        } => {
            let tool = enabled_tools(&sandbox)
                .into_iter()
                .find(|t| t.name == tool_name)
                .with_context(|| format!("unknown tool: {tool_name}"))?;
            print_tool_info(&tool, markdown);
        }
        Command::Run {
            tool_name,
            json_args,
        } => {
            match spadebox_core::call_tool(&sandbox, &tool_name, json_args).await {
                Err(protocol_err) => anyhow::bail!("{protocol_err}"),
                Ok(Err(tool_err)) => {
                    eprintln!("error: {tool_err}");
                    std::process::exit(1);
                }
                Ok(Ok(output)) => print!("{output}"),
            }
        }
    }

    Ok(())
}
