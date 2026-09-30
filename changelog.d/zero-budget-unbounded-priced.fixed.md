- **`--max-cost-usd 0` now refuses a priced task that no token ceiling
  bounds.** An agent, or an `infer:` without `max_tokens`, on a priced
  model passed a zero budget with only a warning, and with a key its first
  call would have been sent and billed. A zero budget now refuses such a
  task before the run starts (`NIKA-1709`, exit 2) and names the task: set
  `max_tokens` (`max_tokens_total` for an agent) so the floor can price
  it. Mock and local models are not metered and never trip the refusal; a
  budget above zero keeps the documented warning and the in-flight guard.
