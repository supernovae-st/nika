- **Realize a bare file name where the observation places it.**
  A request can name a file bare (`orders.csv`) while the project keeps it elsewhere
  (`./data/orders.csv`). The fidelity laws' path law then realizes the name when the compile's
  observation places exactly one file of that name, `permits.fs.read` covers it, and a task opens
  it. A native candidate that reads the observed file is then no longer refused as
  `UNREALIZED PATH`. The refusal stays when there is no observation, when two observed files
  share the name, when only another name is observed, or when no task opens the file. A
  destination keeps its own law. The native door passes its observation to the laws through
  `fidelity::laws_observed`.
