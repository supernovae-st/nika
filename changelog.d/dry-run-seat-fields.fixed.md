- **`nika run --dry-run` refuses what the run's seat refuses.** An `infer:`
  task on a Codex or Claude Code seat over ACP that sets `max_tokens`,
  `temperature`, an enabled `thinking` budget or a `vision:` URL, an
  `agent:` with a `tools:` whitelist on a seat, and an `agent:` declaring
  `images:` with no seat previewed as `dry-run · plan only`, then the run
  refused them before anything was sent (`NIKA-432`, `NIKA-465`). The
  preview now refuses with the same lines `nika check` blocks `run ready`
  on, and exits 2 like its other file refusals.
