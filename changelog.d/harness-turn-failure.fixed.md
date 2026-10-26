- **A turn Codex could not complete no longer settles as a success.** codex-acp returned a
  failed turn (a 400 `model_not_found`, a retry limit exhausted on 429) as answer text and
  `end_turn`, so an `agent:` or `infer:` task on the Codex seat succeeded with the error as its
  output. Nika now advertises codex-acp's `sessionFailure` extension, and the failure the adapter
  then reports fails the task with Codex's own error. An adapter without that extension still
  returns its error as text.
