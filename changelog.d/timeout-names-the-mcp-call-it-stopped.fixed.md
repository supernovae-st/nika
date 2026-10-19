- **A task `timeout:` names the MCP call it stopped.** When a task's
  `timeout:` cut an `mcp:` call, its `NIKA-TIMEOUT-001` error now carries
  `details.stopped_call` = `{tool, call_id, outcome}`, `outcome` being
  `unknown` once the call was sent and `not_sent` when it never was; the
  task's terminal frame repeats it as a `stopped_call` field, and the run's
  settlement lists it in `stopped_calls`, whether the task failed, was
  skipped past or recovered. A task whose call the operator's Stop cut
  carries the same frame field.
