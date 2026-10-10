- **A Session Stop now reaches the Run it handed off.** While a `save & run`
  or a run line executes through the native door, a `stop` command asks that
  Run to stop at its next wave boundary, as a first Ctrl-C does: the work in
  flight completes and is counted, the next tasks are cancelled and the trace
  seals one cancelled terminal. A Stop that arrives before the run child
  exists is applied when it starts (before the spawn, nothing starts). The
  receipt says `stop_requested` until the Run takes the signal and
  `run_stopping` once it has; the snapshot's phase reads `stopping`; the
  result adds `run_stopped` when the trace sealed, `run_aborted` when the Run
  ended without sealing it, and the work snapshot's run gains `sealed`, so an
  interrupted run that stopped is told from one cut mid-flight. A replayed or
  second Stop sends nothing more and never escalates to an abort. A door that
  cannot stop its runs (the resident's job door, for now) still answers
  `run_underway`.
