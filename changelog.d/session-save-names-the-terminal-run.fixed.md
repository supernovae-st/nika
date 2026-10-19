- **A Save says how to run the workflow from a terminal, with the program you run and your
  ceiling.** After a Save, Nika now adds `from a terminal: nika-dev run report.nika
  --max-cost-usd 0.1`: the program is the one Nika was started as (`nika`, or a `nika-…`
  release name), so the command reaches this same binary rather than an older `nika` on the
  path, and the ceiling is the one you saved with the workflow. With no ceiling of yours the
  flag is left out, never filled with a default. A file name with spaces is quoted (NIK-171).
