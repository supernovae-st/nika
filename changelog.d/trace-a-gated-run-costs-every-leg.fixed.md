- **A run that a human gate split in two now costs both of its legs.** The summary after a
  resumed run read « cost · no model usage recorded » although the leg before the gate had made
  a priced call ($0.0438), and the activation's ceiling hint then said « the last run cost
  nothing priced ». A resumed journal now reads what the legs it continued spent from their
  own journals in the same store, following `resumed_from`: « cost · $0.0438 · 1 priced call(s)
  · … · over its 2 legs ». The forecasts that `nika explain` and the ceiling hint read fold the
  resumed leg into the run it continued: the cost is summed, the state is the final leg's, and
  the time is the legs' time.
