- **Intelligence options state each model's prices, context and lifecycle.**
  Every option naming a model now carries typed facts for any host:
  `ModelFacts` gains `input_usd_per_million`, `context_tokens` and `legacy`
  beside the output price, from the route's qualified tariff or else the
  catalogue's list row on a metered route. Unknown stays absent, never
  zero; nothing is selected or called by stating them.
