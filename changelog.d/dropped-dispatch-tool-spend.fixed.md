- **A dropped task now debits the tool spend it had already incurred.** A task cut by its
  `timeout:` or by a `fail_fast` sibling debits, once, the cost reported by tool calls that had
  already completed, including a ready MCP reply it never read. The task stays failed, with no
  output, row or checkpoint credit.
