- **Queued replays cannot own another execution.** The resident claims and
  refuses only queued admissions under the store lease. A delayed duplicate
  cannot resume a paused leg, interrupt the original run, or retire its
  cancellation registration. Known stale entries no longer reopen their
  snapshots. Explicit paused-leg store transitions remain available; this
  does not claim exactly-once external effects or global shutdown liveness.
