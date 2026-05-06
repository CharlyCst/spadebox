use std::env;
use std::sync::Arc;

use anyhow::Context;
use spadebox_core::{DomainRule, HttpVerb, Sandbox, enabled_tools};

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
