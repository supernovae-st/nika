- **An `agent:` loop no longer spends past `max_tokens_total` on `nika:done`
  repairs.** A `result` that misses the declared `schema:` is fed back for a
  repair, and that repair is one more provider request: when the tokens
  already spent meet the budget, the request is not made and the run ends on
  the budget verdict (`NIKA-AGENT-002`), the same gate a tool dispatch
  passes. Previously the repair turns bypassed that gate, so a run could
  exceed the author's budget by up to `DEFAULT_SCHEMA_RETRY_BUDGET` requests.
