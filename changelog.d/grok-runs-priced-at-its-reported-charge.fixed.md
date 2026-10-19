- **Grok runs, priced at the charge xAI reports.** Every xAI model was unpriced at run time: a
  workflow that fetched a page with Grok as its model was refused (« unknown-cost Run supports
  only direct infer and local read/write/jq/assert »), and any other run reported an unknown
  cost, while the model picker showed a price. grok-4.7 and grok-4.3 are now admitted from
  their model pages and settle at the charge each reply reports (`cost_in_usd_ticks`). xAI's
  reasoning is not bounded by `max_tokens` (measured: 1,973 reasoning tokens under a bound of
  40), so a call reserves the model's whole window: a run ceiling needs about $3.40 per
  grok-4.7 call in flight ($2.75 for grok-4.3), while the settled cost stays the real one.
