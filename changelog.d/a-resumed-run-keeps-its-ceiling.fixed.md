- **A resumed run keeps its ceiling.** A run that paused at a gate and was resumed without
  `--max-cost-usd` ran its next leg unbounded (« budget: none journaled »). The run answered in
  the same terminal got its whole ceiling again. Now both continue under what the paused leg's
  ceiling leaves after its spend (`RunFacts::ceiling_left`), and the resumed leg journals that
  budget. A ceiling stated on the resume still stands as given.
