- **A trace directory can be surveyed without fail-open, and a paused run's
  lineage can be read.** `nika-dap`'s `store::survey` keeps every `*.ndjson`
  entry it cannot fold, every doubt about a folded journal (a torn suffix, a
  missing opening frame or run identity, conflicting identities) and every
  listing error; journals carry their identity (`run_id`, `project`), and
  `store::scan` stays its fail-open projection, unchanged. `nika-trace`'s
  `lineage` view folds a survey into which journals continued a paused run:
  `NoneObserved`, a `Chain` with its head, or `Indeterminate` with every
  reason (a fork is never ranked). It is read-only: no authorization, no
  verification, no exactly-once claim.
