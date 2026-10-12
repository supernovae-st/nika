- **An answer cut or interrupted by its provider is never read as finished.** Anthropic's
  `model_context_window_exceeded` and Mistral's `model_length` now count as an output-limit cut,
  and a generation the provider interrupted (DeepSeek's `insufficient_system_resource` and
  `aborted`, the `error` finish of OpenRouter and Mistral, Anthropic's `pause_turn`) is a new
  `StopReason::Interrupted`, where it was read as an unknown, normal end. A limit finish that
  carries a tool call stays a cut on every wire.
