- **A direct seat refuses an `infer:` with `vision:` instead of running it
  without the images.** On a seat route without `run.access.protocol: acp`
  (Codex's `codex exec`, Claude Code's `claude -p`, Grok Build, GitHub Copilot
  CLI), the files were judged and read, then never sent: the command carries
  the prompt, system, model and a `schema:`, never an image. The run now
  refuses the task with `NIKA-432` before the seat is spawned, as the ACP
  route refuses a `vision:` URL, and `nika check` blocks RUN READY with the
  same code, naming `run.access: { via: <seat>, protocol: acp }` where that
  seat's ACP one-shot carries `vision:` files. `temperature`, `max_tokens`
  and `thinking` are still not sent on that route.
