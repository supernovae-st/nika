- **A sketched workflow keeps every result and control the request names.** A workflow composed
  from a sketch can now name its results (`outputs`, each the result of one of its tasks)
  instead of exposing only the last task's output, state an agent's turn bound and its own
  effect-free tools, and state whether a loop stops at its first failure. A stated value is
  emitted exactly; a sketch that states none keeps its previous behaviour, and the record says
  which values were defaults. A misplaced or malformed control, a duplicate or dangling result,
  an effectful agent tool, and a write fed by several inputs without a template are refused at
  the sketch step, where they can still be repaired. A request made of independent copies (each
  source to its own destination) is no longer folded into one read by the plan: it continues in
  the sketch step within the same request budget, or is named when no sketch step is allowed.
