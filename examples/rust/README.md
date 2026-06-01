# Rust example

A simple interactive agent using any OpenAI-compatible API.

```sh
LLM_BASE_URL=https://api.openai.com \
LLM_API_KEY=sk-... \
LLM_MODEL=gpt-4o \
  cargo run -p example --example agent -- /path/to/sandbox
```

Omit `LLM_BASE_URL` to use the local mock server (`tests/mock_openai_api`).
