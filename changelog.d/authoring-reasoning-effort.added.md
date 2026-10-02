- **`nika compile --authoring-reasoning low|high|max` and `nika serve
  --authoring-reasoning` ask an explicit reasoning effort of every
  authoring and decision call.** `NIKA_AUTHORING_REASONING` names one when
  the flag is absent; the flag always wins and never falls back to the
  environment. Any other word, whichever names it, is refused before any
  request (`nika compile` exits 3) or before the Serve listener binds. The
  flag needs a seat to ask it (`--authoring-model`, `--decision-model` or
  both); without one it is a usage error, and the deterministic door never
  reads the environment's word. The effort never moves a cap: calls keep the
  operator's `--authoring-max-tokens` or the default, a truncated answer
  stays a failure, and with a level the decision call asks it under the
  declared authoring cap instead of its compact 256-token request. The level
  is sent only where the model catalog qualifies it: today
  `deepseek/deepseek-v4-pro` (low, high, max) on its exact direct DeepSeek
  endpoint, as `thinking: {type: enabled}` beside `reasoning_effort`.
  Another provider or model, a gateway, a base-URL override and the mock
  refuse before any byte leaves; no fallback runs. Without a level, a
  request keeps the bytes it sent before. Each call's receipt records its
  configured level, the reasoning keys read back from the body it
  dispatched (`unobserved` when no response carried them), the served
  effort as unknown, the reasoning tokens the provider reported (null when
  it reported none) and the model it named. The read-back is the adapter's
  own observation of its serialized request, not a network capture. A
  Session asks the same levels through `NIKA_AUTHORING_REASONING`.
