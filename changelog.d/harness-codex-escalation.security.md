- **A Codex command that asks to leave its sandbox now always pauses.** In `read-only`, Codex runs
  ordinary commands in its sandbox without asking, so every command it asks about asks to run
  with no sandbox at all (`require_escalated`). Its ask carries the command as a string and the
  reason in `_meta.permission.description`, which Nika read as `execute · <prose>`: under
  `permits: { exec: true }` it was allowed, and Codex fetched a site outside its sandbox. It now
  pauses for you on the exec plane (`sandbox · command`) whatever `permits.exec` grants, with the
  command and the reason in the question and in the witness. Every pause question now names the
  command and the reason an ask carries unless its title shows it, on one line a harness cannot
  steer: control characters become spaces and invisible ones (direction overrides, zero-width
  characters) are removed, so a Claude Code title, which is the model's own command, can no
  longer erase itself on your terminal. A long command keeps its end in view (its beginning and
  end around the bytes left out and their SHA-256; whole up to 16 KiB in the witness), and a
  sandbox widening's witness names the command it is for. One `--answer` now decides one ask for
  the task: an attempt that `retry:` replays never reuses it, where a typed 429 failure could
  otherwise replay the session and approve a different command with the same answer.
  `nika check`, `nika explain harness` and docs/usage/harness-agents.md now also state what Codex
  does without asking: it calls its MCP tools marked `readOnlyHint`, opens pages with its hosted
  web tool, writes its own `~/.codex`, starts every configured MCP server and applies your global
  `~/.codex/AGENTS.md` and plugins.
