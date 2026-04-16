---
sidebar_position: 4
---

Move or rename a file or directory, or delete it. Provide 'src' (source) and 'dst' (destination) to move or rename. If 'dst' already exists and 'overwrite' is false (default), the call fails — set 'overwrite' to true to replace it. Set 'create_dirs' to true to create any missing intermediate directories for the destination. To delete instead of moving, omit 'dst' and set 'delete' to true.

### Arguments

**`create_dirs`** (boolean, optional)

If true, create any missing intermediate directories for the destination path.
Defaults to false.

**`delete`** (boolean, optional)

If true and `dst` is omitted, delete `src` (file or directory) instead of moving it.
Required when `dst` is absent, to confirm the deletion is intentional.
Defaults to false.

**`dst`** (string, optional)

Destination path, relative to the sandbox root.
Required unless `delete` is true.

**`overwrite`** (boolean, optional)

If true, overwrite the destination if it already exists.
When overwriting an existing file, the destination must have been read first.
Defaults to false.

**`src`** (string, required)

Source path (file or directory).

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "properties": {
    "create_dirs": {
      "default": false,
      "description": "If true, create any missing intermediate directories for the destination path.\nDefaults to false.",
      "type": "boolean"
    },
    "delete": {
      "default": false,
      "description": "If true and `dst` is omitted, delete `src` (file or directory) instead of moving it.\nRequired when `dst` is absent, to confirm the deletion is intentional.\nDefaults to false.",
      "type": "boolean"
    },
    "dst": {
      "description": "Destination path, relative to the sandbox root.\nRequired unless `delete` is true.",
      "type": [
        "string",
        "null"
      ]
    },
    "overwrite": {
      "default": false,
      "description": "If true, overwrite the destination if it already exists.\nWhen overwriting an existing file, the destination must have been read first.\nDefaults to false.",
      "type": "boolean"
    },
    "src": {
      "description": "Source path (file or directory).",
      "type": "string"
    }
  },
  "required": [
    "src"
  ],
  "title": "MoveParams",
  "type": "object"
}
```
