- **One live line says what Nika does while it prepares or runs.** In the terminal, a turn
  that prepares reads `Preparing · 45s · 12 author calls · Ctrl+C stop`, then the Session's
  own phase words; a workflow the turn runs reads `Running veille-tech.nika · 12s · 6/14
  tasks · now: pick (claude-sonnet-5-5) · $0.012 of $0.25 · Ctrl+C twice leaves` (a Run is
  never stopped there). Its card opens on how it ended: `Ran in 16.5s · 14 tasks · $0.034 ·
  1 model call`. Outside the workspace, whose activity card already says it, a preparation
  that called its author ends with `Prepared in 2m 10s · 35 calls · $0.91`. Every fact is
  one the terminal observed (typed calls, the run's frames, its clock, the Session's known
  cost estimate after the turn); an unknown one is left out, never a placeholder, and the
  facts refresh at most twice a second. Hosts can report typed activity with
  `Seen::activity` and the spend with `Conversation::spend`.
