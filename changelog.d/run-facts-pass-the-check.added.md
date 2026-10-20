- **`nika check` reads the facts of the run.** `${{ run.scheduled_for }}` and
  `${{ run.missed }}`, the closed set of facts the engine tells a run, resolve in
  every value position and in `when:` and never make a data edge. Any other
  `${{ run.X }}` is an unresolved reference (`NIKA-VAR-001`) whose message names
  the two facts; a near miss (`run.schedule_for`) is offered as its rename, a
  typo'd root is offered `run`, and `${ run.missed }` or
  `{{ run.missed }}` without `${{` is caught as for the other namespaces.
