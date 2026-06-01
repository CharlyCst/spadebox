---
sidebar_position: 4
---

Move or rename a file or directory, or delete it. Provide 'src' (source) and 'dst' (destination) to move or rename. If 'dst' already exists and 'overwrite' is false (default), the call fails — set 'overwrite' to true to replace it. Set 'create_dirs' to true to create any missing intermediate directories for the destination. To delete instead of moving, omit 'dst' and set 'delete' to true.

### Arguments

**`src`** (string, required)

Source path (file or directory).

**`dst`** (string, optional)

Destination path, relative to the sandbox root.
Required unless `delete` is true.

**`delete`** (boolean, optional)

If true and `dst` is omitted, delete `src` (file or directory) instead of moving it.
Required when `dst` is absent, to confirm the deletion is intentional.
Defaults to false.

**`overwrite`** (boolean, optional)

If true, overwrite the destination if it already exists.
When overwriting an existing file, the destination must have been read first.
Defaults to false.

**`create_dirs`** (boolean, optional)

If true, create any missing intermediate directories for the destination path.
Defaults to false.

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "MoveParams",
  "type": "object",
  "properties": {
    "src": {
      "description": "Source path (file or directory).",
      "type": "string"
    },
    "dst": {
      "description": "Destination path, relative to the sandbox root.\nRequired unless `delete` is true.",
      "type": [
        "string",
        "null"
      ]
    },
    "delete": {
      "description": "If true and `dst` is omitted, delete `src` (file or directory) instead of moving it.\nRequired when `dst` is absent, to confirm the deletion is intentional.\nDefaults to false.",
      "type": "boolean",
      "default": false
    },
    "overwrite": {
      "description": "If true, overwrite the destination if it already exists.\nWhen overwriting an existing file, the destination must have been read first.\nDefaults to false.",
      "type": "boolean",
      "default": false
    },
    "create_dirs": {
      "description": "If true, create any missing intermediate directories for the destination path.\nDefaults to false.",
      "type": "boolean",
      "default": false
    }
  },
  "required": [
    "src"
  ]
}
```
