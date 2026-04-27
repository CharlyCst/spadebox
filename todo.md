# TODO list

## Read

- [X] Add support for limit to read tool (both bytes and max lines), and optional offset parameter for large files (by lines or bytes?).
      We should also add support for limit to other read tools, such as fetch.
- [ ] Add support for an optional `read_range` parameter.
      Range are expressed in lines, files are 1-indexed, and -1 means read entire document.

## Write

(nothing pending)

## Fetch

- [ ] Make description dynamic, so it can be based on the configured options. E.g, allowed verbs, or http/https. Same for schema description.
- [ ] Add optional sub-agent prompt for summarization. Will require using a callback.
- [ ] Add optional filtering based on a JS script (available only if JS scripting is enabled).
- [ ] Add support for limit (in bytes?). Limit is applied to post-processed documents, or raw if raw is set to true. Add some reasonable default limit (how much? see https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-fetch-tool for claude's recommend limits)? Set to 0 to disable limit?

## Grep

- [ ] Add support for base path (default to "/") (no need for glob, because path can be part of pattern).
- [ ] Add support for case insensitivity.

## Glob

- [ ] Add an optional "depth" parameter to limit the depth of folder explored.
      Needs clarification: the depth should start at the first "**" pattern, are there special edge cases to take care of? Also, probably set a safe default, like 5?

## JS Runtime

- [ ] Add support for node API to read/write files. How to do that? Which API to support (sync/async)? Important: use `cap-std` and the same sandbox as the other file tools.

## General

- [ ] Make a strict separation between ToolResult (for agents), and other results (e.g. when creating the Sandbox or configuring HTTP allowlist).
- [ ] Add a "move" tool to move files and folders, with the ability to delete if there is no target (or a delete option, probably safer).
      Note: we might need to think about how this interacts with the read timestamp.
- [ ] Add a proper Spadebox reset function (for now the best solution is to construct a new Spadebox instance when re-setting the agent context).
- [ ] Doc: explain somewhere that spadebox is designed to not require bash execution tool (but one can be provided in addition if desired).
- [ ] Implement integration tests that use the same source of truth (e.g. input json) across all exposed bindings (Rust, JS, MCP).

---

## Completed

- [X] Prevent writing (or editing) a file if it wasn't read before.
      Note: If the file has been modified by someone else than the agent since last read/write we should also block the write.
- [X] Add fetch tool. We should provide some degree of configuration for the fetch tool, and provide fine-grained domain and verb (GET, POST, etc...) whitelisting.
- [X] Add a default user agent for fetch tool, and make it configurable.
- [X] Fetch: add optional summarization step for HTML (or maybe make it default with an optional raw for HTML content type).

---

## References

Check the following implementations:

- Codex
- Pi Agent
- Goose
- Claude Code (see https://ccunpacked.dev/)
- Claude API (see https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview)

## Other notes

Pi seems to have a `ls` tool, do we also want something like that? Maybe glob is enough.
