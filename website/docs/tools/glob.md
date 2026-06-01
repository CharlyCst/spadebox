---
sidebar_position: 5
---

List files matching a glob pattern. Returns a newline-separated list of relative file paths sorted alphabetically. Use `**` to match across directories (e.g. `**/*.rs` finds all Rust files, `src/**/*.ts` finds TypeScript files under src/). Use 'max_results' to control the result cap (default 300; 0 = unlimited).

### Arguments

**`pattern`** (string, required)

Glob pattern to match file paths against
(e.g. `"**/*.rs"`, `"src/**/*.ts"`, `"**/mod.rs"`).

**`max_results`** (integer, optional)

Maximum number of results to return. Defaults to 300.
Set to 0 to return all matches.

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "GlobParams",
  "type": "object",
  "properties": {
    "pattern": {
      "description": "Glob pattern to match file paths against\n(e.g. `\"**/*.rs\"`, `\"src/**/*.ts\"`, `\"**/mod.rs\"`).",
      "type": "string"
    },
    "max_results": {
      "description": "Maximum number of results to return. Defaults to 300.\nSet to 0 to return all matches.",
      "type": "integer",
      "format": "uint32",
      "minimum": 0,
      "default": 300
    }
  },
  "required": [
    "pattern"
  ]
}
```
