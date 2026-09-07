- **An `agent:` task's `timeout:` now governs every provider call of its
  loop (#1516).** Each turn's request, the `nika:done` repair turns and the
  final tools-off re-ask carry the task budget to the transport deadline,
  the way an `infer:` task already did, so a slow seat is no longer cut at
  the transport's 30 s cloud default on its first turn (measured on 0.118.7:
  agent tasks declaring `timeout: "840s"` failed with `NIKA-INFER-001` at
  30 002 ms while the same seat under `infer:` completed). Without a
  `timeout:` the transport's per-provider default still applies.
