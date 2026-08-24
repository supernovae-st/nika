- **Failed HTTP jobs name a NIKA code.** `GET /v1/jobs/{id}` grows an
  optional `{error:{code,message}}` on `failed`. SSE settled/refused
  frames may carry the same redacted pair. Paths and secret-shaped
  fields stay dropped. Succeeded jobs omit `error`.
