- **A kept run says whether it ran a component of saved bytes.**
  `KeptRun::component_run` reads a journal's frames of the run's own execution:
  its one start must name the hash of the exact saved bytes, and each of the
  component's tasks is settled by that execution's frames alone (`completed`,
  `skipped`, `failed`, `recovered`, `cache_hit`, … or `absent`). The run's exit,
  the last start a journal holds and a child run's task of the same name never
  stand in for it; what the frames cannot attribute is `Unknown`, with why.
