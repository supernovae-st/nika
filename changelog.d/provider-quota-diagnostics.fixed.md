- **Provider quota exhaustion is terminal and sanitized.** Shared HTTP failures
  retain safe status, recognized code/type and Retry-After metadata, distinguish
  exhausted credit from transient rate limits, and discard raw provider prose.
  Usage and billing remain unknown on failures; existing strict wire schemas
  are unchanged. Hermetic tests cover buffered and streaming rejection paths.
