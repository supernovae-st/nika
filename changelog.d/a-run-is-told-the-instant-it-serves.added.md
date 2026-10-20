- **A run is told the instant it serves.** `${{ run.scheduled_for }}` reads the slot a
  `nika arm fire` or a `nika serve` schedule fired the run for, the instant
  `nika test --at <instant>` names, or else the run's own start, as RFC 3339 UTC. The
  run's opening frame journals it with `scheduled_for_source` (`slot` · `at` · `start`),
  and a child workflow the run calls serves the same instant. Under `manqué:
  rattraper-une-fois`, `${{ run.missed }}` reads the silence a catch-up covers, `{ first,
  last, count }` (journaled as `missed`), and null otherwise. A `nika serve` catch-up's job
  carries that silence on its schedule origin as `missed_first` and `missed_count`.
