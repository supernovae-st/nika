- **The transport backs off on a rate-limited or overloaded seat, on the
  run's declared clock.** A 429, 503 or 529 means nothing answered and
  nothing was billed, so the provider layer re-sends the identical request
  at most three times (`Retry-After` honoured up to 30 s, from the header
  or Gemini's body-level `RetryInfo`, else 1 s · 2 s · 4 s), never on
  another 4xx, an exhausted quota, a dropped connection, another 5xx or a
  schema failure; the error surfaces unchanged past the bound so an
  authored `retry:` may still fire. The sleep rides the kernel clock seam:
  the composition injects the run's declared clock, so `run: { clock:
  virtual }` and a seeded run never sleep real seconds. Each attempt is
  boxed so a workflow invoking a workflow fits its stack.
