---
sidebar_position: 1
---

Read the text content of a file. Provide a relative path (e.g. 'src/main.rs' or 'README.md'). Returns the file's content as a UTF-8 string. Use `offset` (1-indexed) and `limit` to read a specific range of lines.

### Arguments

**`path`** (string, required)

Path to the file to read, relative to the sandbox root.

**`limit`** (integer, optional)

Maximum number of lines to return. Omit to read the entire file.

**`offset`** (integer, optional)

1-indexed line number to start reading from. Defaults to 1 (the beginning of the file).

**`max_bytes`** (integer, optional)

Maximum number of bytes to return (applied after `offset`/`limit` windowing).
Defaults to 20 000. Set to 0 to disable.

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "ReadParams",
  "type": "object",
  "properties": {
    "path": {
      "description": "Path to the file to read, relative to the sandbox root.",
      "type": "string"
    },
    "limit": {
      "description": "Maximum number of lines to return. Omit to read the entire file.",
      "type": [
        "integer",
        "null"
      ],
      "format": "uint64",
      "minimum": 0
    },
    "offset": {
      "description": "1-indexed line number to start reading from. Defaults to 1 (the beginning of the file).",
      "type": [
        "integer",
        "null"
      ],
      "format": "uint64",
      "minimum": 0
    },
    "max_bytes": {
      "description": "Maximum number of bytes to return (applied after `offset`/`limit` windowing).\nDefaults to 20 000. Set to 0 to disable.",
      "type": [
        "integer",
        "null"
      ],
      "format": "uint64",
      "minimum": 0
    }
  },
  "required": [
    "path"
  ]
}
```
