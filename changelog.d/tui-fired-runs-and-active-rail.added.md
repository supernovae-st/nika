- **The workspace says what a schedule's trigger fired, and the rail reads « Actif ● » only once
  a fire is proved.** At the opening and after each turn, the workspace reads the Session's history
  of each declared schedule's arm ledger (`SessionRuntime::schedule_history`, a read only) and
  says, once, what changed: « `compiled-workflow.nika` a tourné tout seul : 1 run par son
  déclencheur », its last slot with its exit and its trace, a slot caught up late (« son créneau
  … a été rattrapé à … »), and the slots skipped, the runs a gate parked and the failures. The
  list counts the runs fired under their workflow (« 1 run déclenché · 10-16 06:00 ✔ »). The rail's
  activation field stays declared (◐) until a fire of an installed, active trigger is proved,
  then reads ●, and goes back to declared once the trigger is removed.
