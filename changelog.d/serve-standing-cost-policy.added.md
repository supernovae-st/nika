- **`nika serve --cost-policy` admits runs on an unpriced route with no
  per-run cost review.** A workflow whose model the catalog does not price
  (measured on 0.124.3 with `scaleway/deepseek-v4-flash-0731`) was refused
  under any USD ceiling, and with the ceiling disarmed every job still
  needed `POST /v2/cost-reviews`, a human `approve_once` and one job
  carrying that review, a review living five minutes. The operator now
  records that decision once, at startup: `--cost-policy
  'PROVIDER/MODEL:requests=N,output-tokens=N,timeout=SECONDS,runs=N/PERIOD'`
  (repeatable, requires `--bind`). A job on a named route whose run fits the
  rule runs with no review; a run outside the rule, a route no rule names
  and the run past a full window are still refused before anything is sent.
  The authority is the server's startup composition only: no request field,
  job body or workflow key grants or widens it, and the language is
  unchanged. A rule overrides no ceiling. Without a declared price every
  ceiling the job is under must be absent (`--run-cost-ceiling none`, never
  implied), so scheduled occurrences stay refused; with
  `input-usd=USD,output-usd=USD` (an operator's estimate per million tokens,
  optionally `request-bytes=N`) the run, a scheduled occurrence included,
  is admitted under a present ceiling only when its declared worst case
  fits. The run's cost journal, its trace
  and the new `cost_admission` of the job's settlement
  (`settlement.cost_admission`, `nika/cost-admission@1`) name the
  rule, its bounds, whether the price is `unknown` or `host_declared` and
  the ceiling evidence; a call whose cost is unknown is never recorded as
  zero. The job and its events gain no key of their own, so a published SDK,
  which reads both closed and a settlement open, reads such a job as it
  reads any other. Runs per window are counted in `.nika/standing-cost-runs.json` and
  survive restarts; a second run on the route waits for the first instead of
  failing on the project's cost lease; and a request that left without a
  usable answer still blocks every later run on an unpriced route until it
  is reconciled. Health lists `costStandingPolicyV1` and `GET
  /v1/openapi.json` describes the new fields only on a server that holds a
  policy; beside `--cost-review`, a review of a run the policy admits
  answers `review_required: false` with the rule. Without `--cost-policy`
  every behaviour, refusal and served document is unchanged. A journal row
  written under a rule carries keys an engine before this release does not
  read: after a downgrade, that engine refuses unknown-cost runs in the
  project until the journal is moved aside. A server that holds a policy
  archives a cleared cost journal that nears its 1 MiB read bound to
  `.nika/inference-cost-observations.<digest>.ndjson` before the run that
  needs the room.
