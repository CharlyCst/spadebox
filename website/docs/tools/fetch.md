---
sidebar_position: 7
---

Perform an HTTP request and return the response body as text. The URL must use the http or https scheme. Available methods and domains are determined by the sandbox configuration.

### Arguments

**`body`** (string, optional)

Optional request body (for POST, PUT, PATCH).

**`max_bytes`** (integer, optional)

Maximum number of bytes to return. Defaults to 20 000. Set to 0 to disable.

**`method`** (string, required)

HTTP method to use (e.g. `"GET"`, `"POST"`).

**`raw`** (boolean, optional)

When `true` return the raw response body, otherwise process the content for efficient LLM
consumption (e.g. convert HTML to markdown). Default to `false`

**`url`** (string, required)

The URL to fetch (must use `http` or `https` scheme).

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "properties": {
    "body": {
      "description": "Optional request body (for POST, PUT, PATCH).",
      "type": [
        "string",
        "null"
      ]
    },
    "max_bytes": {
      "description": "Maximum number of bytes to return. Defaults to 20 000. Set to 0 to disable.",
      "format": "uint64",
      "minimum": 0,
      "type": [
        "integer",
        "null"
      ]
    },
    "method": {
      "description": "HTTP method to use (e.g. `\"GET\"`, `\"POST\"`).",
      "type": "string"
    },
    "raw": {
      "default": false,
      "description": "When `true` return the raw response body, otherwise process the content for efficient LLM\nconsumption (e.g. convert HTML to markdown). Default to `false`",
      "type": "boolean"
    },
    "url": {
      "description": "The URL to fetch (must use `http` or `https` scheme).",
      "type": "string"
    }
  },
  "required": [
    "url",
    "method"
  ],
  "title": "FetchParams",
  "type": "object"
}
```
