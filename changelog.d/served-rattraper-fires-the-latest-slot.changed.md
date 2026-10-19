- **A served `rattraper` catch-up fires the latest missed slot once.** As spec 01 states, and as
  the activation's one-ceiling worst spend assumes: a `nika serve` schedule under `manqué:
  rattraper` (the job API's `catch-up`) no longer fires every missed slot, up to 10,000 runs. The
  older slots are skipped, journaled once. A catch-up due while its schedule's previous run still
  runs now waits for that run instead of being skipped as an overlap.
