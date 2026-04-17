# Spadebox &emsp; [![crates.io version]][crates-io] [![NPM version]][npm]

[crates-io]: https://crates.io/crates/spadebox-core
[npm]: https://www.npmjs.com/package/@spadebox/spadebox
[crates.io version]: https://img.shields.io/crates/v/spadebox-core
[NPM version]: https://img.shields.io/npm/v/%40spadebox%2Fspadebox


<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/user-attachments/assets/2f832204-4edc-477a-aec5-f5268aea4756">
    <source media="(prefers-color-scheme: light)" srcset="https://github.com/user-attachments/assets/d0611572-8b35-4d7e-9be1-b1143af419e1">
    <img src="https://github.com/user-attachments/assets/d0611572-8b35-4d7e-9be1-b1143af419e1" width="200px" alt="The Spadebox logo"/>
  </picture>
</div>
<br/>

Spadebox is a set of common tools for AI agents, written in Rust with JavaScript bindings.

Currently, Spadebox includes the following tools:
- `read_file`
- `write_file`
- `edit_file`
- `grep`
- `glob`
- `fetch`

Spadebox uses the [`cap-std` crate](https://github.com/bytecodealliance/cap-std) for file system sandboxing, and domain whitelisting for HTTP requests.

## Usage

### JavaScript

```js
import { SpadeBox } from "@spadebox/spadebox";

const sb = new SpadeBox()
  .enableFiles("/workspace")
  .enableHttp()
  .allow("api.example.com", ["GET", "POST"]);

const tools = sb.tools(); // pass to your LLM as available tools

// dispatch a tool call coming from the model
const result = await sb.callTool("read_file", JSON.stringify({ path: "src/main.rs" }));
```

### Rust

```rust
use spadebox_core::{Sandbox, DomainRule, HttpVerb, enabled_tools, call_tool};

let mut sandbox = Sandbox::new();
sandbox.files.enable("/workspace")?;
sandbox.http
    .enable()
    .allow(DomainRule::new("api.example.com", vec![HttpVerb::Get, HttpVerb::Post])?);

let tools = enabled_tools(&sandbox); // pass to your LLM as available tools

// dispatch a tool call coming from the model
let result = call_tool(&sandbox, "read_file", r#"{"path":"src/main.rs"}"#.into()).await?;
```

### MCP

```sh
# filesystem tools only
spadebox-mcp --files /workspace

# HTTP tools only (allow specific domains and verbs)
spadebox-mcp --allow "api.example.com:GET,POST" --allow "*.cdn.example.com:GET"

# both
spadebox-mcp --files /workspace --allow "api.example.com:GET"
```
