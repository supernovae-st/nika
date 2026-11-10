- **`nika serve` answers what a workflow's declared ACP route offers,
  without a prompt.** `GET /v1/access-offers/{name}` (capability
  `accessSessionOffer` on `/health`) admits a served workflow by name, reads
  its `run.access`, and, when the file selects ACP sessions (`via` under
  `protocol: acp`), opens one session per shape its tasks run in and per
  model they resolve to: the route's one-shot completion profile for
  `infer:` tasks, its agent session for `agent:` tasks. Each session is
  opened as a run opens it, selects the file's model for that session only,
  lists the models, the exact reasoning efforts and the service tiers the
  route has for that model, and ends: no effort, tier or mode is set and the
  path holds no `session/prompt`, so an offer uses no turn. The answer is a
  `nika/access-offer@1` document: the file's declared route, then one row
  per session with `observed_at` and `fresh_until`. A model the session does
  not list is a row state, and its efforts and tiers are then left
  `unobserved` instead of being read from another model. A session that
  gives no offer is a row with the code a run would meet (`NIKA-1803`,
  `NIKA-1804`, `NIKA-1805`). Rows are kept 60 seconds (15 when unavailable),
  one reading runs at a time, a ninth concurrent request answers `503
  access_offer_busy`, and every listed value reaches the client as a bounded
  token. The request names a served workflow and nothing else, and the
  answer authorizes no run: `POST /v1/jobs` still selects, reads back and
  refuses on its own session. In the library, `nika_harness::session_offer`
  reads one session and `nika_service_execution::offer` names the sessions
  of a workflow.
