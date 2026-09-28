- **Serve's cost-review door speaks version 2 for fans and authored retries.**
  A server started with `--cost-review` now also serves `POST /v2/cost-reviews`,
  `GET /v2/cost-reviews/{id}` and `POST /v2/cost-reviews/{id}/decision`, and
  health lists `costReviewV2`. A version-2 review carries the typed `dispatch`
  bound: the total of physical requests, the requests in flight at once, the
  authored-retry law and one row per task. The witness covers it, and one
  approval confirms exactly those limits for one `POST /v1/jobs`. A fan of
  zero items needs no review and sends nothing. Version 1 keeps its closed
  document and refuses a fan or an authored retry, naming the version-2 route.
  The versions never cross (ids, decisions and idempotency keys stay with the
  version that created them).
