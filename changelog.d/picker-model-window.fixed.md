- **The intelligence picker states a model's own context window, and how far a tiered price
  holds.** Claude Haiku 5.5 read "100k context" because its tariff, which prices prompts up
  to 100,000 tokens, stood in for its million-token window. An option's `context_tokens` is
  now the model's window (its provider row when a source is recorded for it, else the
  tariff's, else the list row's), and `ModelFacts` gains `prices_up_to_tokens` when a tariff
  prices only a cheaper part of that window: the TUI reads "from $0.10 in / $0.50 out per
  MTok (≤100k tokens) · 1M context".
