<div align="center">
  <h1>SpadeBox</h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/user-attachments/assets/2f832204-4edc-477a-aec5-f5268aea4756"/>
    <img src="https://github.com/user-attachments/assets/d0611572-8b35-4d7e-9be1-b1143af419e1" width="200px" alt="The Spadebox logo" />
  </picture>
  <br/>
  Sandboxed tools and JS runtime for your AI agents

  [![crates.io version]][crates-io] [![NPM version]][npm] [![PyPI version]][pypi]
</div>

[crates-io]: https://crates.io/crates/spadebox
[npm]: https://www.npmjs.com/package/@spadebox/spadebox
[pypi]: https://pypi.org/project/spadebox/
[crates.io version]: https://img.shields.io/crates/v/spadebox-core
[NPM version]: https://img.shields.io/npm/v/%40spadebox%2Fspadebox
[PyPI version]: https://img.shields.io/pypi/v/spadebox

<br/>

SpadeBox is a set of sandboxed tools and a JS runtime for AI agents, written in Rust with JavaScript and Python bindings.
Focus on your domain-specific tools and harness — give your agent SpadeBox for the rest.

<div align="center">

| Category       | Tools                                                          |
|----------------|----------------------------------------------------------------|
| Files          | `read_file`, `write_file`, `edit_file`, `move`, `grep`, `glob` |
| Network        | `fetch`                                                        |
| Code execution | `js_repl`, `js_exec`                                           |

</div>

## Features

- **Lightweight sandboxing:** SpadeBox uses the [`cap-std` crate](https://github.com/bytecodealliance/cap-std) for file system sandboxing, and domain allowlisting for HTTP requests.
  The JS engine is based on [boa](https://boajs.dev/), and uses the same sandboxing policies.
- **Configurable:** Pick only the tools you need for your application: files, network, or code execution, or any combination of those.
- **No `bash` tool:** SpadeBox has been designed to work well even _without_ a `bash` tool.
  The tools are designed to cover all the basic operations an agent needs without having to shell-out.
  If you would like your agent to use `bash` you can provide your own `bash` tool in addition to SpadeBox.
- **Native function in JS runtime:** Expose native functions to the SpadeBox JS runtime to allow your agents to programmatically interact with your application.
  Compatible with all supported host language bindings.
- **Secret management for HTTP requests:** Register credentials for specific HTTP domains and get a token that can be safely shared with your agent. Spadebox replaces the token by the actual secret within HTTP requests to the target domain.
- **Default limits to preserve context:** SpadeBox's tools try to safeguard your agent context, with default limits to tool outputs and HTML-to-markdown conversion.


## Usage

(checkout out the [documentation](https://spadebox.github.io/docs/about) for more)

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
