- **A settled run draws no task running, and a lost journal says why.** A task whose end frame
  was drained (its output over the stream's 1 MiB bound) kept `◐ fetch_stripe` on the run's
  card and list although the settlement said `11/11 tasks ok`. Once a run settles no task
  reads running: one still drawn running reads `settled ok` when the settlement counts every
  task ok, and `no end received` otherwise. When the run's evidence is `lost`, its header now
  says why when the stream told it (a step's output its trace could not keep either) and
  `reason not reported` when nothing did (NIK-195; the 1 MiB trace bound itself is NIK-131).
