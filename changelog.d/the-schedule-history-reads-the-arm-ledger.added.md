- **The Session reads each schedule's history, and journals its removal.**
  `SessionRuntime::schedule_history()` returns every schedule `nika.yaml` declares, with how many
  runs its arm ledger records as fired, skipped, paused, failed or disarmed, its last decided slot
  and whether this Mac holds its unit. Reading it creates, locks and repairs nothing.
  « désactive-le » now journals a `disarmed` decision in the schedule's ledger and says so.
  Installing a trigger from the conversation installs that schedule's unit only: a schedule kept
  declared stays without one.
