- **A direct seat refuses an `infer:` with `vision:` instead of running it
  without the images.** On a seat route without `run.access.protocol: acp`
  (Codex's `codex exec`, Claude Code's `claude -p`, Grok Build, GitHub
  Copilot CLI), the files were judged and read, then never sent: the command
  carries the prompt, system and model (and a `schema:`, except on Copilot),
  never an image. The run now refuses the task with `NIKA-432` before the
  seat is spawned, as the ACP route refuses a `vision:` URL, and `nika
  check` blocks RUN READY with the same code. Both name `run.access: { via:
  <seat>, protocol: acp }` where that seat's ACP one-shot carries `vision:`
  files and the rest of the task; a task that also sets what that session
  refuses (`temperature`, `max_tokens`, `thinking`, a `vision:` URL) is
  pointed at an API route, with those fields named. `temperature`,
  `max_tokens` and `thinking` are still not sent on the direct route.
