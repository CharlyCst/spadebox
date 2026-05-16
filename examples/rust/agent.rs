//! Example agent using any OpenAI-compatible chat completions API.
//!
//! Usage:
//!     cargo run -p example --example agent -- <absolute-sandbox-path>
//!
//! Environment variables:
//!     LLM_BASE_URL  Base URL of the chat completions API (e.g.: https://api.openai.com)
//!     LLM_API_KEY   API key
//!     LLM_MODEL     Model name

use std::env;
use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use spadebox::SpadeBox;

// --- Configuration ---

fn base_url() -> String {
    env::var("LLM_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8324".to_owned())
        .trim_end_matches('/')
        .to_owned()
}
fn api_key() -> String {
    env::var("LLM_API_KEY").unwrap_or_default()
}
fn model() -> String {
    env::var("LLM_MODEL").unwrap_or_else(|_| "none".to_owned())
}

// --- Colors ---

const RESET: &str = "\x1b[0m";
const BLUE: &str = "\x1b[34m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const GRAY: &str = "\x1b[90m";
const CYAN: &str = "\x1b[36m";

// --- Types ---

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "role", rename_all = "snake_case")]
enum Message {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ToolCall {
    id: String,
    #[serde(default = "default_tool_call_type")]
    r#type: String,
    function: ToolCallFunction,
}

fn default_tool_call_type() -> String {
    "function".to_owned()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ToolCallFunction {
    name: String,
    arguments: String,
}

// --- API ---

async fn chat(
    client: &reqwest::Client,
    messages: &[Message],
    tools: &Value,
) -> anyhow::Result<Message> {
    let body = serde_json::json!({
        "model": model(),
        "messages": messages,
        "tools": tools,
        "tool_choice": "auto",
    });
    let res = client
        .post(format!("{}/v1/chat/completions", base_url()))
        .header("Authorization", format!("Bearer {}", api_key()))
        .json(&body)
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        anyhow::bail!("API error {status}: {text}");
    }
    let data: Value = res.json().await?;
    Ok(serde_json::from_value(data["choices"][0]["message"].clone())?)
}

// --- Agent loop ---

// [snippet: agent-loop]
async fn run_turn(
    sb: &SpadeBox,
    client: &reqwest::Client,
    messages: &mut Vec<Message>,
    tools: &Value,
) -> anyhow::Result<()> {
    loop {
        let response = chat(client, messages, tools).await?;
        let tool_calls = match &response {
            Message::Assistant { tool_calls, .. } => tool_calls.clone(),
            _ => None,
        };
        messages.push(response.clone());

        let calls = match tool_calls {
            Some(calls) if !calls.is_empty() => calls,
            _ => {
                if let Message::Assistant { content: Some(text), .. } = response {
                    println!("\n{CYAN}Agent:{RESET} {text}\n");
                }
                return Ok(());
            }
        };

        for call in calls {
            let name = &call.function.name;
            let args = &call.function.arguments;
            println!("\n{BLUE}[call]{RESET} {GRAY}{name}({args}){RESET}");

            let result = sb.call_tool(name, args).await?;
            let tag = if result.is_error { format!("{RED}[error]{RESET}") } else { format!("{GREEN}[ok]{RESET}") };
            println!("{tag} {GRAY}{}{RESET}", result.output);

            messages.push(Message::Tool {
                tool_call_id: call.id.clone(),
                content: result.output,
            });
        }
    }
}
// [/snippet]

// --- Main ---

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let sandbox_path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: cargo run -p example --example agent -- <absolute-sandbox-path>");
        std::process::exit(1);
    });

    // [snippet: setup]
    let sb = SpadeBox::new()
        .enable_files(&sandbox_path)?
        .enable_js()
        .enable_http()
        .allow("*", &["GET", "HEAD"])?;
    // [/snippet]

    // [snippet: tool-definitions]
    let tools: Value = sb
        .tools()
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": serde_json::from_str::<Value>(&t.input_schema).unwrap(),
                },
            })
        })
        .collect();
    // [/snippet]

    let client = reqwest::Client::new();
    let mut messages = vec![Message::System {
        content: "You are a helpful agent, help the user and use your tools as appropriate."
            .to_owned(),
    }];

    println!("Agent ready. Sandbox: {sandbox_path}");
    println!("Endpoint: {}, Model: {}", base_url(), model());
    println!("Type your request, Ctrl+D to exit.\n");

    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim().to_owned();
        if line.is_empty() {
            continue;
        }
        messages.push(Message::User { content: line });
        if let Err(e) = run_turn(&sb, &client, &mut messages, &tools).await {
            eprintln!("Error: {e}");
        }
    }

    Ok(())
}
