- **An infer answer interrupted by its provider fails as a provider call.** When the provider
  reports it interrupted the generation (`StopReason::Interrupted`), the answer is never the task
  output: the task fails `NIKA-INFER-001`, transient, naming the provider's own word, and the
  task's `retry:` decides. An answer cut at the output cap that parses, even one that satisfies
  `schema:`, still fails `NIKA-INFER-005`.
