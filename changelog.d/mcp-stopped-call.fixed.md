- **An MCP call the run stops waiting for is cancelled, not left running.**
  When a task's `timeout:` cut an `invoke` of an `mcp:` tool, its call kept
  running on its worker until the 120 s call timeout, holding the server's
  pipe so every other call to that server waited behind it, and the server
  was never told; the operator's Stop (Ctrl-C, SIGTERM, a Session's Stop)
  waited for such a call to finish. The call now carries a stop: one not yet
  written is never sent, and one in flight is cancelled with
  `notifications/cancelled` while its session stays open for the next call.
  Under the operator's Stop the task fails `NIKA-CANCEL-001`, never retried,
  its `error.details.stopped_call` naming `{tool, call_id, outcome}` with
  `outcome` `not_sent` or `unknown` (it was sent, so the server may have
  acted), and the run's settlement lists every such call as `stopped_calls`
  (`{task, tool, call_id, outcome}`), whether its task failed, was skipped
  past or recovered. Other in-flight work still completes at the wave
  boundary. A reply that already arrived now wins over a stop, a server
  request that reuses the call's id is no longer taken for its reply, and a
  stopped call's late reply no longer counts against the next call's byte
  budget.
