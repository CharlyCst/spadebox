<div align="center">
  <h1>SpadeBox</h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/user-attachments/assets/2f832204-4edc-477a-aec5-f5268aea4756"/>
    <img src="https://github.com/user-attachments/assets/d0611572-8b35-4d7e-9be1-b1143af419e1" width="200px" alt="The Spadebox logo" />
  </picture>
  <br/>
  Shovels for your AI agents

  [![crates.io version]][crates-io] [![NPM version]][npm] [![PyPI version]][pypi]
</div>

[crates-io]: https://crates.io/crates/spadebox
[npm]: https://www.npmjs.com/package/@spadebox/spadebox
[pypi]: https://pypi.org/project/spadebox/
[crates.io version]: https://img.shields.io/crates/v/spadebox-core
[NPM version]: https://img.shields.io/npm/v/%40spadebox%2Fspadebox
[PyPI version]: https://img.shields.io/pypi/v/spadebox

<br/>

Spadebox is a set of tools for lightweight AI agents, written in Rust with JavaScript and Python bindings.
Focus on your domain-specific tools — give your agent SpadeBox for the rest.

<div align="center">

| Category       | Tools                                                          |
|----------------|----------------------------------------------------------------|
| Files          | `read_file`, `write_file`, `edit_file`, `move`, `grep`, `glob` |
| Network        | `fetch`                                                        |
| Code execution | `js_repl`, `js_exec`                                           |

</div>

## Features

**Lightweight sandboxing:**

SpadeBox uses the [`cap-std` crate](https://github.com/bytecodealliance/cap-std) for file system sandboxing, and domain whitelisting for HTTP requests.
The JS engine is based on [boa](https://boajs.dev/), and uses the same sandboxing policies.

**No `bash` tool:**

SpadeBox has been designed to work well even _without_ a `bash` tool.
SpadeBox exposes a JS engine for code execution, and provides the common file system operations for your agent to be effective without needing to shell-out.
If you would like your agent to use `bash` you can provide your own `bash` tool in addition to SpadeBox.

**Just the tools:**

SpadeBox does one thing: it provides tools.
Write your own agent loop or use your favorite framework, SpadeBox doesn't have an opinion on that.

**Small perks:**

SpadeBox's tools include small quality of life features for your AI agents.
Default tool output limit and HTML to markdown conversion protect the agent's context, while read-before-write rule limits risks of data losses.

## Usage

### JavaScript

```js
import { SpadeBox } from "@spadebox/spadebox";

const sb = new SpadeBox()
  .enableFiles("/workspace")
  .enableHttp()
  .allow("api.example.com", ["GET", "POST"])
  .enableJs();

const tools = sb.tools(); // pass to your LLM as available tools

// dispatch a tool call coming from the model
const result = await sb.callTool("read_file", JSON.stringify({ path: "src/main.rs" }));
```

### Rust

```rust
use spadebox_core::{Sandbox, DomainRule, HttpVerb, enabled_tools, call_tool};

let mut sandbox = Sandbox::new();
sandbox
    .enable_fs("/workspace")?
    .enable_http()
    .allow(DomainRule::new("api.example.com", vec![HttpVerb::Get, HttpVerb::Post])?)
    .enable_js();

let tools = enabled_tools(&sandbox); // pass to your LLM as available tools

// dispatch a tool call coming from the model
let result = call_tool(&sandbox, "read_file", r#"{"path":"src/main.rs"}"#.into()).await?;
```

### Python

```python
from spadebox import SpadeBox

sb = (SpadeBox()
    .enable_files("/workspace")
    .enable_http()
    .allow("api.example.com", ["GET", "POST"])
    .enable_js())

tools = sb.tools()  # pass to your LLM as available tools

# dispatch a tool call coming from the model
result = sb.call_tool("read_file", '{"path": "src/main.rs"}')
```

### MCP

```sh
# filesystem tools only
spadebox-mcp --files /workspace

# HTTP tools only (allow specific domains and verbs)
spadebox-mcp --allow "api.example.com:GET,POST" --allow "*.cdn.example.com:GET"

# JavaScript REPL only
spadebox-mcp --js

# all tools
spadebox-mcp --files /workspace --allow "api.example.com:GET" --js
```
