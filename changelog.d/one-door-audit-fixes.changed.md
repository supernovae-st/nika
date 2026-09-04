- **The freeze audit's fixes (One Door · OD-F12).** Ten proven slices, each
  the smallest correct owner of a gap the audit found:
  (1) The resident projects the run's settlement whole (ADR-128):
  `execution.settled` and `execution.cancelled` carry `settlement` (`status` ·
  `cause` · `elapsed_ms` · `tasks` · `spend` · `error`), the SSE frame
  projects it, the OpenAPI names it (`RunSettlement`), the job's status is the
  settlement's own state, and the SSE terminal set is the record's
  (`JobStatus::is_settled`).
  (2) A retry finds its job before the registry is read again (ADR-132 · G1):
  `POST /v1/jobs` looks the `Idempotency-Key` up BEFORE capturing, so a
  lost-response retry replays the ORIGINAL job even after the served workflow
  changed, vanished or went red, and never executes changed bytes; the
  resident's own `schedule:` key namespace is refused to a manual caller.
  (3) The resident stamps its stores after it holds the lease (ADR-132): a
  second `nika serve` that loses the server lease never rewrites the live
  resident's writer stamp; opening a store never stamps it.
  (4) Words never above the proof: a digest a caller sends is a
  caller-supplied integrity digest, never an « attestation » (ADR-131
  amended); the resident's status words are proven equal to the settlement's
  (ADR-130).
  (5) `nika check --sdk-snapshot` is public: the SDK's producer of the
  snapshot body, still an adapter of `check`, never a verb.
  (6) The session decides a proposal only once it landed (ADR-133): a stale
  apply leaves it undecided (a retry reads `wrong_state`, never a false
  `already_consumed`), and the check after an apply is the same composed
  judgment `nika check` makes.
  (7) The local door says what evidence it left and never resumes a run in
  flight (ADR-129): `run_settled.evidence` (`sealed` · `unsealed` · `lost` ·
  `none`), and `nika run --resume` on a trace whose writer is alive refuses
  (ENV · the writer named).
  (8) Unknown cost is never a zero on a human line (ADR-128 · #1278): the
  closing card and the live meter say `unmetered` or `unpriced (N calls)`
  instead of `$0.00`; `nika trace session` says « no spend metered »; the cost
  replay prints the journaled qualifier.
  (9) `nika-tui-core` reads graph format 3, pinned to `nika-graph` (ADR-130):
  a node knows its `kind`, a cleanup unit is never seated on the board, and
  the wasm doors refuse a format they do not speak, naming both numbers.
  (10) The seat words never above the rung (ADR-134): `nika doctor` says « its
  login command reports signed in », the welcome line and the escape hatch say
  a seat is « present (its login is judged at run) », a local endpoint that
  answered a ping « answered ».
