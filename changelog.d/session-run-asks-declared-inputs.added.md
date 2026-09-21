- **A run asks the declared inputs it needs before it runs.** In the native
  session, « run it » on a workflow that declares a required input with no
  default no longer dies at launch (NIKA-1708): each such value is asked on
  its own `reply ›` line (key `input.<name>`), a `name=value` written on the
  run line is honoured, the values are bound as `--var` pairs, and only then
  is the run requested. `cancel` drops the request, an empty line is not a
  value, and the pending input is its own state, never an authoring answer
  nor a consent.
