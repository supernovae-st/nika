- **A cloud inference call is no longer cut at 30 s by a deadline the workflow never declared.**
  An `infer:` task without `timeout:` gave a buffered cloud call a total deadline of 30 s (300 s
  for a local model), so a legitimate answer from a reasoning model or a long prompt failed with
  NIKA-INFER-001 (provider 408) and the Run wrote nothing. Every buffered call without a task
  `timeout:` now gets 600 s, local or cloud: the provider transport's own bound on a connection
  that delivers nothing. A task `timeout:` still sets its own bound, streaming keeps its
  idle-read guard, and the timeout message names the deadline that applied.
