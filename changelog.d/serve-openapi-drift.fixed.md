- **The `nika serve` OpenAPI document declares what the server answers.**
  The trace verify response's `tier`, the 412 schedule error's
  `error.currentRevision`, the schedule 422 `{findings}` body (kept as the
  published SDK reads it) and the 404 of a by-name `POST /v1/check` and
  `POST /v1/jobs` were sent but undeclared. `GET /v1/workflows/{name}` now
  says a name containing `/` is sent with literal separators, and the
  cancel route states its fixed 5-second grace before a run is
  interrupted.
