"""
Example agent using any OpenAI-compatible chat completions API.

Usage:
    uv run agent.py <absolute-sandbox-path>

Environment variables:
    LLM_BASE_URL  Base URL of the chat completions API (e.g.: https://api.openai.com)
    LLM_API_KEY   API key
    LLM_MODEL     Model name
"""

import json
import os
import sys
import urllib.request
import urllib.error
from typing import Any

from spadebox import SpadeBox

# --- Configuration ---

BASE_URL = os.environ.get("LLM_BASE_URL", "http://localhost:8324").rstrip("/")
API_KEY = os.environ.get("LLM_API_KEY", "")
MODEL = os.environ.get("LLM_MODEL", "none")

# --- Colors ---

RESET = "\x1b[0m"
BLUE = "\x1b[34m"
GREEN = "\x1b[32m"
RED = "\x1b[31m"
GRAY = "\x1b[90m"
CYAN = "\x1b[36m"

# --- Validate args ---

if len(sys.argv) < 2:
    print("Usage: uv run agent.py <absolute-sandbox-path>", file=sys.stderr)
    sys.exit(1)

sandbox_path = sys.argv[1]

# --- Setup SpadeBox ---

sb = SpadeBox()
sb.enable_files(sandbox_path).enable_js().enable_http().allow("*", ["GET", "HEAD"])

tools = [
    {
        "type": "function",
        "function": {
            "name": t.name,
            "description": t.description,
            "parameters": json.loads(t.input_schema),
        },
    }
    for t in sb.tools()
]

# --- API ---


def chat(messages: list[dict[str, Any]]) -> dict[str, Any]:
    payload = json.dumps(
        {"model": MODEL, "messages": messages, "tools": tools, "tool_choice": "auto"}
    ).encode()
    req = urllib.request.Request(
        f"{BASE_URL}/v1/chat/completions",
        data=payload,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {API_KEY}",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req) as res:
            data = json.loads(res.read())
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"API error {e.code}: {e.read().decode()}") from e
    return data["choices"][0]["message"]


# --- Agent loop ---


def run_turn(messages: list[dict[str, Any]]) -> None:
    while True:
        response = chat(messages)
        messages.append(
            {
                "role": "assistant",
                "content": response.get("content"),
                "tool_calls": response.get("tool_calls"),
            }
        )

        tool_calls = response.get("tool_calls") or []
        if not tool_calls:
            content = response.get("content")
            if content:
                print(f"\n{CYAN}Agent:{RESET} {content}\n")
            return

        for call in tool_calls:
            name = call["function"]["name"]
            args = call["function"]["arguments"]
            print(f"\n{BLUE}[call]{RESET} {GRAY}{name}({args}){RESET}")

            result = sb.call_tool(name, args)
            tag = f"{RED}[error]{RESET}" if result.is_error else f"{GREEN}[ok]{RESET}"
            print(f"{tag} {GRAY}{result.output}{RESET}")

            messages.append(
                {"role": "tool", "tool_call_id": call["id"], "content": result.output}
            )


# --- Main ---

SYSTEM_PROMPT = (
    "You are a helpful agent, help the user and use your tools as appropriate."
)

messages: list[dict[str, Any]] = [{"role": "system", "content": SYSTEM_PROMPT}]

print(f"Agent ready. Sandbox: {sandbox_path}")
print(f"Endpoint: {BASE_URL}, Model: {MODEL}")
print("Type your request, Ctrl+D to exit.\n")

try:
    while True:
        try:
            line = input("> ")
        except EOFError:
            break
        if not line.strip():
            continue
        messages.append({"role": "user", "content": line})
        try:
            run_turn(messages)
        except Exception as e:
            print(f"Error: {e}", file=sys.stderr)
except KeyboardInterrupt:
    pass
