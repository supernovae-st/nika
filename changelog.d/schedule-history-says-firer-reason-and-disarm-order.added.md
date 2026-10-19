- **A schedule's history says its firer, its last skip's reason and its catch-ups.**
  `ScheduleHistory` now says whether a resident `nika serve` holds the project's server lease
  (`serve_running`), and when the unit was last disarmed and how many runs fired since
  (`last_disarmed_at`, `fired_since_disarm`). `LastDecision` now carries a skip's reason
  (`missed:1`, `overlap`) and the slots a catch-up run answered. Both types have `new()`
  constructors. The unit's presence is asked of the Session's trigger host.
  `nika_cadence::ledger::decisions` returns each decision with its instant, reason and slots.
