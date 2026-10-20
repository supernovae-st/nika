- **A run refused before it starts now settles as refused, with its reason.** It stayed « run
  (starting) » in the aside and the run pane. Now the leg is named « run (refused) », its pinned
  row reads « refused · NIKA-1709 · … », and the run pane says « refused before its start · … ·
  nothing ran ». For other doors, a `RunSink` gains `refused(why)`, a method that does nothing
  by default, told by the run's story when the stream's first document is the check's refusal
  or the error envelope.
