- **Retire the signal listener when its run ends.** A terminal session joins
  each run's listener on return or unwind, including after cancellation. An
  old listener can no longer interpret a later run's first Ctrl-C as its own
  second signal and abort the process without settling the current run.
