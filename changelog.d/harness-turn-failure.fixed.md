- **A turn Codex could not complete no longer settles as a success.** codex-acp returned a
  failed turn (a 400 `model_not_found`, a retry limit exhausted on 429) as answer text and
  `end_turn`, so an `agent:` or `infer:` task on the Codex seat succeeded with the error as its
  output. Nika now advertises the `sessionFailure` extension, which codex-acp and
  claude-agent-acp implement, and the failure the adapter then reports fails the task with its
  own error: transient (`NIKA-1804`) when the adapter offers a retry, an expired sign-in when it
  asks you to sign in again, a refusal (`NIKA-1805`) otherwise. `nika explain` now describes
  both codes for such a failed turn. An adapter without that extension still returns its error
  as text.
