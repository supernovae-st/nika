- **The resident preserves the runtime's actual result across cancellation,
  pause and replay.** Cancelling an active job acknowledges the request;
  the runtime may still succeed or fail, and a missing result after grace
  is interrupted rather than fabricated cancellation. Queued cancellation
  and execution claim have one leased winner. Durable GET, idempotent
  admission replay and SSE expose the stored settlement; a pause closes
  the observation with that leg's outputs and receipt while the job stays
  resumable. Ordinary event append cannot inject a pause boundary, and
  malformed pause results are rejected without committing a mutation.
