- **Explicit provider refusals stop before repair or tool dispatch.**
  Buffered OpenAI and Anthropic refusals stop `agent:` and `infer:` after
  recording incurred usage, even alongside valid structured content.
  Ordinary text without a refusal signal keeps its existing behavior.
