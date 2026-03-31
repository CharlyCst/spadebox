# SpadeBox

SpadeBox is a Rust library to provide common tools for AI agents.
For instance, SpadeBox provides tools to read, write, or update files.
SpadeBox uses the `cap-std` crate to provide lightweight sandboxing.

SpadeBox can be used through:
- The native Rust library
- As an MCP server
- In JavaScript through the napi-rs bindings

## How To

- Run all tests: `just test`

## Security

Tools should NEVER be able to escape the sandbox.
In particular, all file-system access should go through `cap-std`.
