- **A harness probe takes its wrapper's grandchild with it.** `nika doctor`
  (and every census) probes the `codex` seat through its npm wrapper
  (`node …/bin/codex-acp -c model=…`), which forks the real adapter binary
  as a grandchild; the probe killed the wrapper alone and the grandchild
  lived on. Measured 2026-09-22 on a machine whose editor extension polls
  `nika doctor`: ~1.7 orphaned `codex-acp` per second, the process table
  full within the hour (« fork: Resource temporarily unavailable » for
  every shell). Every probe child is now its own process group and the
  group ends with the probe, on every path (the handshake answer read, a
  timeout, an exit); a test forks a `sleep 600` grandchild behind a fake
  wrapper and proves it is gone after the probe.
