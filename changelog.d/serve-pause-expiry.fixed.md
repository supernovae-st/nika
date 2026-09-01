- **A `pauseUntil` in the past wakes the schedule under `nika serve`.**
  The serve planner judged `active: false` as paused forever and never
  read the declared bound. It now evaluates the expiry the arm-fire way —
  the date strictly before the decision instant's own civil date — so an
  expired pause plans as active and fires, while a future one stays
  visibly paused.
