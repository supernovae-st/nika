- **A run's ceiling now reads the same way wherever it appears.** Before a run, the
  Session wrote « ceiling $0.1 (saved with the workflow) » and the line below it wrote « ceiling
  $0.10 ». A ceiling under a cent showed as « $0.00 », which is wrong. Every surface that names a
  ceiling (the run's announcement, the proposal's asked run, the project facts, a run's result,
  `nika check` on a project) now writes the amount as you set it: `$0.10`, `$0.25`, `$0.001`,
  `$2`. Every significant digit is kept, cents show wherever there is a fraction, and no amount is
  rounded to zero.
