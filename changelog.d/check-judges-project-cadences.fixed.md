- **`nika check nika.yaml` judges the beats as `nika arm` and `nika serve`
  do.** It read the project file's shape only, so a beat whose cadence has
  no `TZ=` zone, names an unknown zone, breaks the cron or readable grammar,
  carries a `décalage` other than `hash`, a `jusqu_au` that is not a date or
  a workflow path with a `..` segment passed `check` and was refused only
  when someone armed it. `check` now applies the arming door's own laws
  (the same function `nika arm` uses) and names every law broken, each with
  its beat, its code and its fix; `--json` lists one finding per law.
