# TODO list

Items we should address:
- [X] Prevent writing (or editing) a file if it wasn't read before.
      Note: If the file has been modified by someone else than the agent since last read/write we should also block the write.
- [ ] Add support for limit to read tool (both bytes and and max lines), and optionnal offset parameter for large file (by lines or bytes?).
- [ ] Add support for selecting which tools to include. Maybe we could start with presets? Like "Files" and "FilesReadOnly", where "Files" would be read/write/edit/grep/glob, and "FilesReadOnly" would be read/grep/glob.
- [ ] Add fetch tool. We should provide some degree of configuration for the fetch tool, and provide fine-grained domain and verb (GET, POST, etc...) whitelisting.

Code maintenance:
- [ ] Write unit tests for each tools using tempfile
- [ ] Comment code in all toos, like for the grep tool

## References:

Check the following implementation:

- Codex
- Pi Agent
- Goose
- Claude Code (see https://ccunpacked.dev/)
