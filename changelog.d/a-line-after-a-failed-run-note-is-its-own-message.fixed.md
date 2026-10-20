- **A line typed after a failed run's diagnosis is the person's own message.** When a run the
  person's words authorized failed, Nika's note started a turn the model answered, but the
  proposal it ran still counted as waiting: the person's next line was read as that proposal's
  answer and moved into its reply, so the request ended on the model's own message. Claude Opus
  5.5 refused it (« does not support assistant message prefill ») and the turn failed. Once the
  model has answered such a note, nothing waits on the proposal any more.
