---
sidebar_position: 5
---

List files matching a glob pattern. Returns a newline-separated list of relative file paths sorted alphabetically. Use `**` to match across directories (e.g. `**/*.rs` finds all Rust files, `src/**/*.ts` finds TypeScript files under src/).

### Arguments

**`pattern`** (string, required)

Glob pattern to match file paths against
(e.g. `"**/*.rs"`, `"src/**/*.ts"`, `"**/mod.rs"`).

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "properties": {
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
