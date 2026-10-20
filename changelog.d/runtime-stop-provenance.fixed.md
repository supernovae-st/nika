- **Operator-stopped MCP tasks and native agent loops retain their control outcome and every
  actual stopped-call witness across fan-out, fail-fast and timeout.** Ordinary authored recovery
  and completed in-flight work retain their meaning; stopped work cannot become a successful
  resume checkpoint. Ready MCP replies keep their physical Stop and reported cost facts when a
  stopped sibling drops their future before its final poll. A stopped batch counts those costs
  once, without turning unread work into a successful checkpoint.
