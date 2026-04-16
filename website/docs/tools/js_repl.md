---
sidebar_position: 8
---

Evaluate JavaScript code and return the result as a string. The session is persistent: variables and functions defined in one call are available in subsequent calls.

### Arguments

**`code`** (string, required)

JavaScript source code to evaluate.

The evaluation runs in a persistent session: variables, functions, and
any other state defined in previous calls are available in subsequent ones.

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "description": "Parameters for the `js_repl` tool.",
  "properties": {
    "code": {
      "description": "JavaScript source code to evaluate.\n\nThe evaluation runs in a persistent session: variables, functions, and\nany other state defined in previous calls are available in subsequent ones.",
      "type": "string"
    }
  },
  "required": [
    "code"
  ],
  "title": "JsReplParams",
  "type": "object"
}
```
