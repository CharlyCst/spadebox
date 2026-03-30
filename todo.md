# TODO list

Items we should address:
- [ ] Prevent writing (or editing) a file if it wasn't read before.
      Note: If the file has been modified by someone else than the agent since last read/write we should also block the write.
- [ ] The write tool should also create intermediate directories if needed (and this should be said as part of the description).
- [ ] Add support for limit to read tool (both bytes and and max lines), and optionnal offset parameter for large file (by lines or bytes?).

## References:

Check the following implementation:

- Codex
- Pi Agent
- Goose
