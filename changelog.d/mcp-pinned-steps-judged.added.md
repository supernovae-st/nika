- **A pinned `mcp:` step is judged against its approved schema.** Once the
  project registers the server, approved its pins under the command,
  arguments or URL the registry names now, and the pins hold the tool,
  `nika compile` judges an `invoke:` step's literal arguments against the
  pinned input schema (an argument a template builds is wired, the run's to
  judge) and records each verdict on `decision.mcp`, without reaching the
  server. A registry re-pointed after its approval keeps the hold until the
  server is approved again, as the run's own verification requires. The
  served compile door judges the same step on the server project's pins and
  records the verdict, but keeps the step held: that server runs no MCP
  step. Any other answer keeps the source-only hold, in words that say what
  to do; permits stay the only authority over what a step may do.
  `nika check` advises, without blocking, on a registered server whose pins
  were never approved or do not hold the tool.
