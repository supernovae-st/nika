- **`nika serve --cost-review` seats a one-time cost-review door.** A job whose
  exact route has an unknown USD cost can now run on a server: `POST
  /v1/cost-reviews` frames one fresh review with the same evaluator as `nika run`
  and holds the project's cost lease. `POST /v1/cost-reviews/{id}/decision` records one
  explicit `approve_once` or `decline`. A single `POST /v1/jobs` carrying the
  review id and its witness runs it, after every bound fact is re-observed. A review never creates a
  job, a run or a file by itself, lives 300 seconds, and survives no restart.
  Health lists `costReviewV1` only when the door is seated. The server's per-run
  ceiling stays a hard cap: `--run-cost-ceiling <USD|none>` sets it, `0` is a
  valid binding veto (a zero server ceiling no longer refuses at startup), and
  only an explicit `none` lets an unknown cost be approved. Unreviewed jobs now go
  through the shared evaluator, which binds the declared-free observer and
  still refuses an unknown-cost route before the worker starts.
