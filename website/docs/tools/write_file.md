---
sidebar_position: 2
---

Write text content to a file. Provide a relative path (e.g. 'src/main.rs') and the full UTF-8 content to write. Creates the file if it does not exist, or overwrites it entirely if it does. If the file already exists, it must be read first — attempting to overwrite without a prior read will return an error. Set 'create_dirs' to true to create any missing intermediate directories automatically. To create a directory without writing a file, end the path with '/' (e.g. 'src/utils/') and set 'create_dirs' to true — content is ignored in that case.

### Arguments

**`path`** (string, required)

Path to the file to write, relative to the sandbox root.
To create a directory instead of a file, end the path with '/' (e.g. 'src/').

**`content`** (string, optional)

Content to write (UTF-8). Ignored when creating a directory.

**`create_dirs`** (boolean, optional)

If true, create any missing intermediate directories before writing.
Required when the parent directory does not yet exist.
When the path ends with '/', creates the directory (and any parents) without writing a file.

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "WriteParams",
  "type": "object",
  "properties": {
    "path": {
      "description": "Path to the file to write, relative to the sandbox root.\nTo create a directory instead of a file, end the path with '/' (e.g. 'src/').",
      "type": "string"
    },
    "content": {
      "description": "Content to write (UTF-8). Ignored when creating a directory.",
      "type": "string",
      "default": ""
    },
    "create_dirs": {
      "description": "If true, create any missing intermediate directories before writing.\nRequired when the parent directory does not yet exist.\nWhen the path ends with '/', creates the directory (and any parents) without writing a file.",
      "type": "boolean",
      "default": false
    }
  },
  "required": [
    "path"
  ]
}
```
