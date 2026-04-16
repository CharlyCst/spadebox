---
sidebar_position: 6
---

Search file contents for a regex pattern (ripgrep). Returns matching lines with their file path and line number. Use 'glob' to restrict the search to specific file types (e.g. '**/*.rs'). Use 'context_lines' to include N surrounding lines around each match.

### Arguments

**`context_lines`** (integer, optional)

Number of context lines to show before and after each match.
Defaults to 0 (matched lines only).

**`glob`** (string, optional)

Optional glob to restrict which files are searched
(e.g. `"**/*.rs"`, `"src/**/*.ts"`). Matches all files when omitted.

**`pattern`** (string, required)

Regex pattern to search for (e.g. `"fn main"`, `"TODO.*fixme"`).

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "properties": {
    "context_lines": {
      "default": 0,
      "description": "Number of context lines to show before and after each match.\nDefaults to 0 (matched lines only).",
      "format": "uint32",
      "minimum": 0,
      "type": "integer"
    },
    "glob": {
      "description": "Optional glob to restrict which files are searched\n(e.g. `\"**/*.rs\"`, `\"src/**/*.ts\"`). Matches all files when omitted.",
      "type": [
        "string",
        "null"
      ]
    },
    "pattern": {
      "description": "Regex pattern to search for (e.g. `\"fn main\"`, `\"TODO.*fixme\"`).",
      "type": "string"
    }
  },
  "required": [
    "pattern"
  ],
  "title": "GrepParams",
  "type": "object"
}
```
