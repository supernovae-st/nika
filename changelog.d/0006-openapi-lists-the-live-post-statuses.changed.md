- **OpenAPI lists the live POST statuses.** `GET /v1/openapi.json` names
  400, 408, 409, 413, 415, 503, and 507 on `POST /v1/jobs` next to
  200/202/401/422. `info.description` names that `POST /v1/run` is
  absent. `GET /v1/jobs/{id}/status` stays `{status}` only; diagnosis
  lives on `GET /v1/jobs/{id}` and SSE.
