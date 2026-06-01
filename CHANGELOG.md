# Changelog

All notable changes to this project will be documented in this file.

## [0.5.0] - 2026-06-01

### Added

- **`expose_js_func`**: Expose native Rust/JS/Python functions to the embedded JavaScript runtime, including async
  function support.
- **Public Rust API**: New `spadebox` crate with a user-friendly high-level interface.
- **`grep`/`glob` default result limit**: Tools now cap output and include a `<warning>` when results are truncated.
- **`js_exec` ES module support**: Scripts are evaluated as ES modules, enabling top-level `import` statements.
- **CLI `--root` parameter**: Choose the sandbox root directory from the command line.
- **JS subagent example**: Example showing how to use Spadebox to spawn subagents from the JS runtime.

## [0.4.0] - 2026-05-09

### Added

- **Python bindings**: New Python bindings for Spadebox.
- **`js_exec` tool**: Execute JavaScript files in the embedded runtime. Available via Rust library, JS bindings, and MCP
  server.
- **CLI**: New `spadebox` CLI for testing tools from the command line, including `tools info <tool_name>`.
- `js` runtime: Support for `fetch`.
- `js` runtime: Basic `console` APIs (`console.log`, etc.).
- `js` runtime: Support for `import` and `require` of the `fs` module.
- `js` runtime: Synchronous file API (`readFileSync`, `writeFileSync`, etc.).

## [0.3.0] - 2026-05-04

### Added

- **`move` tool**: Move or rename files and directories, with support for creating intermediate directories.
- **`js_repl` tool**: Execute JavaScript snippets in an embedded REPL. Available via Rust library, JS bindings, and MCP
  server.
- `fetch`: `max_bytes` parameter to cap response size.
- `fetch`: Configurable `User-Agent` header.
- `fetch`: HTML-to-markdown conversion for cleaner LLM-friendly output.
- `fetch`: Filter out `<script>` and `<style>` tags from HTML responses.
- `read`: `limit` and `offset` parameters for paginated file reads.

### Fixed

- `read`, `write`, `edit`, `glob`: Accept paths with a leading `/`.

## [0.2.0] - 2026-04-17

### Added

- **`fetch` tool**: HTTP fetch with URL allowlist support. Available via Rust library, JS bindings, and MCP server.
- `fetch`: Configurable HTTP settings (base URL, allowlist rules).
- Support for listing which tools are currently enabled.

### Changed

- Filesystem tools are now configurable: root directory and per-tool enable/disable.
- File and HTTP configuration are now exposed through the MCP server.
