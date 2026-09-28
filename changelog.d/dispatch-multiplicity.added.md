- **An unknown-cost Run can fan out and retry inside one finite, reviewed bound.**
  A `for_each` over a literal list or an input/const array (the operator's value
  before the default) and an authored `retry.max_attempts` are now reviewed
  instead of refused. The one fresh question shows each task's breakdown (items
  × authored attempts × calls, schema re-asks included), the original total of
  physical requests and the requests in flight at once. The approval confirms
  exactly those limits. Every reservation, sent or not, counts against the
  total and takes one in-flight slot atomically. A received 429 or 503 may be
  followed only by the workflow's own authored retry, inside the total; any
  other failure, a timeout or a cancelled send stops every further request.
  The transport itself never resends, and usage and USD cost stay unknown. A
  fan of zero items asks nothing and sends nothing. A count only the run
  decides (a task output), `on_error`, exec, agent and nested workflows stay
  refused. Session reviews and single-attempt Runs keep their exact behaviour.
