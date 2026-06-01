---
sidebar_position: 5
---

List files matching a glob pattern. Returns a newline-separated list of relative file paths sorted alphabetically. Use `**` to match across directories (e.g. `**/*.rs` finds all Rust files, `src/**/*.ts` finds TypeScript files under src/). Use 'max_results' to control the result cap (default 300; 0 = unlimited).

### Arguments

**`max_results`** (integer, optional)

Maximum number of results to return. Defaults to 300.
Set to 0 to return all matches.

**`pattern`** (string, required)

Glob pattern to match file paths against
(e.g. `"**/*.rs"`, `"src/**/*.ts"`, `"**/mod.rs"`).

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "properties": {
    "max_results": {
      "default": 300,
      "description": "Maximum number of results to return. Defaults to 300.\nSet to 0 to return all matches.",
      "format": "uint32",
      "minimum": 0,
      "type": "integer"
    },
    "pattern": {
      "description": "Glob pattern to match file paths against\n(e.g. `\"**/*.rs\"`, `\"src/**/*.ts\"`, `\"**/mod.rs\"`).",
      "type": "string"
    }
  },
  "required": [
    "pattern"
  ],
  "title": "GlobParams",
  "type": "object"
}
```
