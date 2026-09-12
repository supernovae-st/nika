- **Explicit agent completion (breaking).** From the 0.120 development train,
  an `agent:` task whose effective whitelist grants `nika:done` requires an
  explicit completion call. Text-only plans receive feedback to continue or
  finish within the existing turn and token budgets. Without the sentinel
  grant, natural text completion remains available; exclusions are respected.
  This prevents a plan-only run from falsely reporting success (#1519).
