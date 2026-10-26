- **A Codex command that asks to leave its sandbox now always pauses.** In `read-only`, Codex runs
  ordinary commands in its sandbox without asking, so every command it asks about asks to run
  with no sandbox at all (`require_escalated`). Its ask carries the command as a string and the
  reason in `_meta.permission.description`, which Nika read as `execute · <prose>`: under
  `permits: { exec: true }` it was allowed, and Codex fetched a site outside its sandbox. It now
  pauses for you on the exec plane (`sandbox · command`) whatever `permits.exec` grants, with the
  command and the reason in the question and in the witness. Every pause question now names the
  command and the reason an ask carries when its title does not. `nika check`, `nika explain
  harness` and docs/usage/harness-agents.md now also state what Codex does without asking: it
  calls its MCP tools marked `readOnlyHint`, opens pages with its hosted web tool, writes its own
  `~/.codex`, starts every configured MCP server and applies your global `~/.codex/AGENTS.md`
  and plugins.
