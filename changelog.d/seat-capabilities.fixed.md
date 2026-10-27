- **`check` and the run agree on what a Codex or Claude Code seat can carry.** `check` said
  `run ready` for `schema:` and `vision:` on a seat while the run refused them. An `infer:`
  task over ACP now carries a `schema:`: the prompt carries the schema and Nika validates the
  answer, and each repair is a new one-turn session that receives the earlier answer and the
  errors (`NIKA-INFER-002` after the budget). Its `vision:` files ride as ACP image blocks when
  the application declares image prompts at `initialize` and the file carries an image
  signature. An `agent:` task's `schema:` is validated on the seat's final message, with no
  repair turn. What a seat still cannot carry (`temperature`, `max_tokens`, an enabled
  `thinking` budget, a `vision:` URL on `infer:`, a `tools:` whitelist on `agent:`) is a `check`
  blocker, and `check` no longer suggests `max_tokens` or `max_tokens_total` on a seated task.
