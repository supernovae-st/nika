- **A line typed before a question is shown never answers it.** In the plain session loop,
  a line typed while Nika worked (compiling, or running a workflow) answered the next
  question as soon as it appeared: a `yes` typed during a compile applied a proposal the
  person never saw, and a `y` typed while a run waited approved its gate and let its write
  happen. On a terminal, that typeahead is now discarded before every question waits: the
  proposal's `apply? ›`, a gate's `answer ›`, an authoring `reply ›` and the intelligence
  choice. The in-process gate ask of `nika run` on a terminal does the same, and a terminal
  it cannot drain leaves the run at its durable pause with its resume line taught. The idle
  `nika ›` prompt keeps a line typed early, the cost question keeps its own drain, and a
  pipe keeps its scripted lines.
