- **`nika check` accepts cumulative agent budgets above a model's context
  window.** `max_tokens_total` bounds usage across turns, while the context
  window bounds one request. Comparing them rejected valid multi-turn
  budgets, including 120,000 tokens on a 64,000-token seat (#1518).
  Per-request `infer.max_tokens` capacity checks and the agent's runtime
  token accounting remain unchanged; this does not certify that each
  growing agent request fits the seat.
