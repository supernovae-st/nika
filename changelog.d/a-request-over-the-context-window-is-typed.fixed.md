- **A request over the model's context window is told apart from other refusals.** A
  provider's documented overflow code (`context_length_exceeded`) used to be dropped, so
  the failure read as a generic HTTP 400. It is now a typed context overflow that names
  the model's window when the catalog knows it. On a wire without such a code (Anthropic),
  a 400 `invalid_request_error` counts as an overflow only when Nika's own estimate of the
  request reaches that window; the provider's wording never decides. The overflow is
  never retried as transient, and 429, 503 and 529 stay transient rejections. An answer
  that stopped because the context window was full (Anthropic
  `model_context_window_exceeded`, Mistral `model_length`) stays cut, as before: never a
  normal end, its tool calls never run. It now also carries a window-full marker, set too
  when a provider reports more input than the model's window holds, so a conversation
  can compact before asking again instead of re-sending the same full window.
