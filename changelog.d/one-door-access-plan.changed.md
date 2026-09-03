- **One access plan is the execution authority.** `nika run` resolves the
  access plan once per attempt (the effective models with their verbs,
  this machine's provider and seat rows, the `--access` pin) and then
  executes exactly that plan: the seat comes from the plan, the announce,
  the `--dry-run` preview, `check --json`'s `access_plan` rows and the
  boot manifest are projections of it, and every `infer:`/`agent:` task
  routes by its own lane. A model with no ready path refuses before the
  first task (`NIKA-1800`, with the witnesses) instead of failing inside
  a task with a provider error; an ACP-only seat never serves an `infer:`
  lane; one seat holds a run. On the shipped 0.116.2 the announce could
  name a seat the run never rode and the run could dial the API with a
  dead key while the seat was ready (the census-B divergence).
