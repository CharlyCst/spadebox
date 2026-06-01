---
sidebar_position: 6
---

Search file contents for a regex pattern (ripgrep). Returns matching lines with their file path and line number. Use 'glob' to restrict the search to specific file types (e.g. '**/*.rs'). Use 'context_lines' to include N surrounding lines around each match. Use 'max_matches' to control the result cap (default 100; 0 = unlimited).

### Arguments

**`context_lines`** (integer, optional)

Number of context lines to show before and after each match.
Defaults to 0 (matched lines only).

**`glob`** (string, optional)

Optional glob to restrict which files are searched
(e.g. `"**/*.rs"`, `"src/**/*.ts"`). Matches all files when omitted.

**`max_matches`** (integer, optional)

Maximum number of matches to return across all files. Defaults to 100.
Set to 0 to return all matches (use with care on large repos).

**`pattern`** (string, required)

Regex pattern to search for (e.g. `"fn main"`, `"TODO.*fixme"`, `"(?i)path"`).

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
    "max_matches": {
      "default": 100,
      "description": "Maximum number of matches to return across all files. Defaults to 100.\nSet to 0 to return all matches (use with care on large repos).",
      "format": "uint32",
      "minimum": 0,
      "type": "integer"
    },
    "pattern": {
      "description": "Regex pattern to search for (e.g. `\"fn main\"`, `\"TODO.*fixme\"`, `\"(?i)path\"`).",
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
