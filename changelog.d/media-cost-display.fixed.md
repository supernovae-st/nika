- **Paid media no longer reads as free or doubles its code on the cost
  lines.** `nika arm` printed `NIKA-1709 · NIKA-1709 · refusing to start: …`
  for a beat a cost floor refuses, a paid media call included, and broke the
  line before `(budget_floor)`; the readiness blocker now carries the
  sentence alone, the code once on its own field, in the text and in
  `--json`. The audited line of `nika check`, and the green answer of the
  MCP `nika_check` tool, read `est out ≤$0.0000` for a workflow whose only
  spend is a priced media call; they now add the calls and the least they
  cost (`+ 1 paid media call from $0.0006`), counted as the `--max-cost-usd`
  floor counts them: a literal `for_each` multiplies its call, while a call
  behind a `when:` gate, over an expression `for_each` or made by a child
  workflow is named as one only the run counts. Metered calls are named
  beside unpriced ones.
