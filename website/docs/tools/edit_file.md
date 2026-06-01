---
sidebar_position: 3
---

Replace text within a file. Reads the file, finds the exact string provided in 'old_string', and replaces it with 'new_string'. By default the string must appear exactly once — include enough surrounding context in 'old_string' to make it unique. If the string appears multiple times and you want to replace all of them, set replace_all to true. Always read the file before editing to ensure 'old_string' matches the current content exactly.

### Arguments

**`path`** (string, required)

Path to the file to edit, relative to the sandbox root.

**`old_string`** (string, required)

Exact string to search for. Must appear exactly once unless replace_all is true.
Include enough surrounding context to uniquely identify the location.

**`new_string`** (string, required)

String to replace it with.

**`replace_all`** (boolean, optional)

If true, replace every occurrence instead of requiring exactly one. Defaults to false.

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "EditParams",
  "type": "object",
  "properties": {
    "path": {
      "description": "Path to the file to edit, relative to the sandbox root.",
      "type": "string"
    },
    "old_string": {
      "description": "Exact string to search for. Must appear exactly once unless replace_all is true.\nInclude enough surrounding context to uniquely identify the location.",
      "type": "string"
    },
    "new_string": {
      "description": "String to replace it with.",
      "type": "string"
    },
    "replace_all": {
      "description": "If true, replace every occurrence instead of requiring exactly one. Defaults to false.",
      "type": "boolean",
      "default": false
    }
  },
  "required": [
    "path",
    "old_string",
    "new_string"
  ]
}
```
