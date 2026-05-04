# Changelog

All notable changes to this project will be documented in this file.

## [0.3.0] - 2026-05-04

### Added
- **`move` tool**: Move or rename files and directories, with support for creating intermediate directories.
- **`js_repl` tool**: Execute JavaScript snippets in an embedded REPL. Available via Rust library, JS bindings, and MCP server.
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
