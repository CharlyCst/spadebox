# SpadeBox

SpadeBox is a Rust library to provide common tools for AI agents. For instance, SpadeBox provides tools to read, write,
or update files. SpadeBox uses the `cap-std` crate to provide lightweight sandboxing.

SpadeBox can be used through:

- The native Rust library (`rust/`)
- As an MCP server (`crates/spadebox-mcp`)
- In JavaScript via napi-rs bindings (`js/`)
- In Python via PyO3 bindings (`python/`)

## Codebase Overview

- `crates/spadebox-core` — Core library, source of truth for all tool logic. Each tool lives in `src/tools/<name>.rs`
  and implements the `Tool` trait.
- `crates/spadebox-mcp` — MCP server. Thin wrapper that derives tool names, descriptions, and schemas from
  `spadebox-core` at runtime.
- `crates/spadebox-cli` — Human-friendly CLI interface to `spadebox-core`.
- `rust/` — Public Rust binding (re-exports `spadebox-core`'s public API).
- `js/` — JavaScript bindings via napi-rs. `src/lib.rs` is the Rust source; `index.d.ts` is auto-generated on build.
- `python/` — Python bindings via PyO3/maturin.
- `examples/` — Example projects for each language binding.
- `skill/` — Agent skill files (source of truth). Symlinked into `.claude/skills/` for Claude Code.

## How To

- Run all tests: `just test`
- Build the whole project: `just build`

## Security

Tools should NEVER be able to escape the sandbox. In particular, all file-system access should go through `cap-std`.
