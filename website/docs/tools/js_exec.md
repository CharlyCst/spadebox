---
sidebar_position: 9
---

Execute a JavaScript file in a fresh runtime and return an empty string on success, or an error message if the script throws. No state is shared with the JS REPL — each call starts from a clean context.

### Arguments

**`path`** (string, required)

Path to the JavaScript file to execute.

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "JsExecParams",
  "type": "object",
  "properties": {
    "path": {
      "description": "Path to the JavaScript file to execute.",
      "type": "string"
    }
  },
  "required": [
    "path"
  ]
}
```
