- **An answer the provider cut at its output limit now fails instead of passing as the
  output.** An `infer:` task whose provider stopped at `max_tokens` used to return the
  truncated text as a success (a model that reasons by default can spend most of the cap
  on reasoning: 4,096 of 4,096 output tokens, 3,860 of them reasoning, the answer cut
  mid-sentence). It now fails with `NIKA-INFER-005` (not transient), before any `schema:`
  validation or repair. The run record's `details` carry the text received
  (`partial_output`) and the provider's stop reason, and an `on_error:` recovery can use
  them. An agent's final message cut the same way fails with `NIKA-AGENT-006`. A blank
  cut answer keeps its earlier failure: `NIKA-INFER-004` on a text task, the schema check
  that names the token limit under `schema:`.
