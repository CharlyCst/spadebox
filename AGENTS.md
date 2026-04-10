# SpadeBox

SpadeBox is a Rust library to provide common tools for AI agents.
For instance, SpadeBox provides tools to read, write, or update files.
SpadeBox uses the `cap-std` crate to provide lightweight sandboxing.

SpadeBox can be used through:
- The native Rust library
- As an MCP server
- In JavaScript through the napi-rs bindings

## Codebase Overview

- `crates/spadebox-core` — Rust library, source of truth for all tool logic and documentation. Each tool lives in `src/tools/<name>.rs` and implements the `Tool` trait.
- `crates/spadebox-mcp` — MCP server. Thin wrapper: derives tool names, descriptions, and schemas directly from `spadebox-core` at runtime.
- `js/` — JavaScript bindings via napi-rs. `src/lib.rs` is the Rust source; `index.d.ts` is auto-generated on build. Convenience methods (`readFile`, `writeFile`, etc.) wrap the core tools directly.
- `skill/` — Agent skill files (source of truth). Symlinked into `.claude/skills/` for Claude Code.

## How To

- Run all tests: `just test`
- Build the whole project: `just build`

## Security

Tools should NEVER be able to escape the sandbox.
In particular, all file-system access should go through `cap-std`.
