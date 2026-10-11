- **A Codex-led Session conversation can write its candidate.** Codex 0.156.1 asks before each
  Nika tool that is not read-only, and the ask reaches the client only through its
  `tool_call_mcp_elicitation` feature, which the Codex profile turned off with every other
  feature. Codex then asked through `request_user_input` instead; codex-acp 1.13.1 answers that
  itself with no answer, because Nika advertises no form elicitation, and Codex reads no answer
  as the user cancelling the call. So `candidate_write` was cancelled without Nika ever being
  asked. The conversation profile now keeps that feature on, and the spawn-time read-back
  requires it on, so each ask reaches Nika as `session/request_permission` and Nika allows its
  own tool once. The one-shot completion profile and `codex exec` keep it off.
