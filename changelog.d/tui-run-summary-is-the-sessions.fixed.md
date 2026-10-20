- **One summary per run: the Session's.** After a run, the conversation read two summaries of it,
  the TUI's « ✔ Exécuté en 1.3s · 2 tâches exécutées » and its files, then the Session's
  « Terminé · `copie.nika` · 1.3 s · 2 étapes exécutées » with its cost and ceiling. The Session's
  block is now the one summary. The TUI's card under the run keeps only its steps, and only when
  they say more: a step failed (its row in the failure's red), or a model was called (each step
  with its model and its own cost). A run with no model and no failure has no card; the run
  object (F6) and the run's story (F2) keep the whole detail.
