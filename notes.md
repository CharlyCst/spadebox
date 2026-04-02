Here's a concise but complete spec:

---

# Spadebox — Project Specification

## Overview

**Spadebox** is a Rust library providing sandboxed, capability-safe agent tools for use in AI agent pipelines. It exposes a small set of common tools (file I/O, regexp search, code execution) behind a strict sandbox boundary, with bindings for Python and JavaScript/TypeScript.

## Goals

- Provide a safe, jailed execution environment for AI agent tool calls
- Core logic in Rust; thin bindings for Python (PyO3) and JavaScript (napi-rs)
- Async-first design using Tokio
- Structured, FFI-safe error types

## Repository Layout

```
spadebox/
├── spadebox-core/       # Rust core library
├── spadebox-python/     # PyO3 bindings (maturin)
├── spadebox-js/         # napi-rs bindings + auto-generated .d.ts
└── Cargo.toml           # workspace root
```

## Sandbox Model

The central type is `Sandbox`, constructed once with a jail root path using `cap-std`'s `Dir`. All tool operations accept `&Sandbox` instead of raw paths — no ambient filesystem access occurs after construction.

```rust
pub struct Sandbox {
    root: cap_std::fs::Dir,
}
```

On Linux 5.6+, `cap-std` uses the `openat2` syscall with `RESOLVE_BENEATH`, giving kernel-level enforcement that no path component (including symlinks) can escape the jail root. On older kernels and macOS, `cap-std` falls back to correct userspace resolution. Hardlink mitigation is a deployment concern (mount the jail root on a separate filesystem).

## Tools

### File I/O

- `read(path) -> Vec<u8>` — reads a file relative to the jail root
- `write(path, data: &[u8])` — writes (creates or truncates) a file relative to the jail root
- `edit(path, old: &str, search: &str, replacement: &str)` — targeted string replacement within a file; reads the current contents, replaces the first occurrence of `old` with `replacement`, and writes back. Errors if `old` is not found or is ambiguous (appears more than once). More efficient than a full write for small changes.
- All paths resolved via `cap-std`; symlink escapes and `..` traversal are rejected at the kernel level

### Regexp Search

Uses the ripgrep library crates (`grep-searcher`, `grep-regex`, `ignore`) — **not** a subprocess. Directory walking is done via `cap-std`'s `read_dir` recursively. Files are opened via `cap-std`, then converted to `std::fs::File` via `into_std()` and passed to `grep-searcher`'s `search_file()`. This avoids TOCTOU at search time since ripgrep operates on an already-open fd.

- `search(pattern: &str, glob: &str) -> Vec<Match>` — returns file path, line number, and matched line

### JavaScript Execution

Uses the `boa` crate for in-process JS execution. No subprocess, no Node.js dependency. Execution runs with a configurable timeout (via Tokio's `time::timeout`). Host functions (e.g. `readFile`, `writeFile`) are optionally exposed into the Boa context and route back through the `Sandbox`, preserving jail enforcement.

- `execute_js(code: &str, timeout: Duration) -> Result<JsValue>`

## Bindings

**Python** (`spadebox-python`): built with PyO3 and distributed as a wheel via `maturin`. Async methods are exposed as Python coroutines via `pyo3-asyncio`.

**JavaScript/TypeScript** (`spadebox-js`): built with `napi-rs`. TypeScript definitions are auto-generated. The native `.node` addon works with Node.js, Deno, and Bun.

## Error Model

A single `TooldError` enum with structured variants (not stringly-typed) that survive the FFI boundary cleanly. Variants include at minimum: `EscapeAttempt`, `NotFound`, `PermissionDenied`, `Timeout`, `JsError`, `IoError`.

## Out of Scope (v1)

- WASM target
- Network access tools
- Process/subprocess execution
- Hardlink mitigation (deployment concern)
