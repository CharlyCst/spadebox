---
sidebar_position: 7
---

Perform an HTTP request and return the response body as text. The URL must use the http or https scheme. Available methods and domains are determined by the sandbox configuration.

### Arguments

**`url`** (string, required)

The URL to fetch (must use `http` or `https` scheme).

**`method`** (string, required)

HTTP method to use (e.g. `"GET"`, `"POST"`).

**`body`** (string, optional)

Optional request body (for POST, PUT, PATCH).

**`headers`** (object, optional)

Optional HTTP headers to include in the request (e.g. `{"Authorization": "Bearer token"}`).

**`raw`** (boolean, optional)

When `true` return the raw response body, otherwise process the content for efficient LLM
consumption (e.g. convert HTML to markdown). Default to `false`

**`max_bytes`** (integer, optional)

Maximum number of bytes to return. Defaults to 20 000. Set to 0 to disable.

### Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "FetchParams",
  "type": "object",
  "properties": {
    "url": {
      "description": "The URL to fetch (must use `http` or `https` scheme).",
      "type": "string"
    },
    "method": {
      "description": "HTTP method to use (e.g. `\"GET\"`, `\"POST\"`).",
      "type": "string"
    },
    "body": {
      "description": "Optional request body (for POST, PUT, PATCH).",
      "type": [
        "string",
        "null"
      ]
    },
    "headers": {
      "description": "Optional HTTP headers to include in the request (e.g. `{\"Authorization\": \"Bearer token\"}`).",
      "type": [
        "object",
        "null"
      ],
      "additionalProperties": {
        "type": "string"
      }
    },
    "raw": {
      "description": "When `true` return the raw response body, otherwise process the content for efficient LLM\nconsumption (e.g. convert HTML to markdown). Default to `false`",
      "type": "boolean",
      "default": false
    },
    "max_bytes": {
      "description": "Maximum number of bytes to return. Defaults to 20 000. Set to 0 to disable.",
      "type": [
        "integer",
        "null"
      ],
      "format": "uint64",
      "minimum": 0
    }
  },
  "required": [
    "url",
    "method"
  ]
}
```
