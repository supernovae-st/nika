- **Provider defaults and nicknames follow the served model ids.** OpenRouter uses
  `anthropic/claude-sonnet-4.6` and `anthropic/claude-haiku-4.5` with their sourced
  limits; `deepseek/chat` aliases `deepseek-flash`, while the explicit retired wire
  id remains unchanged. OpenAI defaults to `gpt-6.1-sol` and `gpt-6-luna`; xAI
  defaults to `grok-4.7` and `grok-4.3`, with the first-run suggestion naming
  `xai/grok-4.3`. Retired grok-3 prices are removed, and the human catalogue lists
  each aliased wire once. Groq defaults retain their models.
