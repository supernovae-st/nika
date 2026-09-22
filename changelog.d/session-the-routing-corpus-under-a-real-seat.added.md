- **The routing corpus under a real seat (the Arena seam).** An ignored
  live test routes a JSONL corpus (`NIKA_ROUTING_CORPUS`: id · state ·
  line · expected · optional automation and last prompt · milestone rows)
  through the real `ReasonerClassifier` over `NIKA_ROUTING_MODEL`, prints
  one receipt line per row and writes them to `NIKA_ROUTING_RECEIPT`; the
  routing milestone rows must route exactly, the rest is measured. The
  corpus is authored beside the lane (45 rows · EN/FR/ES · typos · mixed
  lines · gate and question phases).
