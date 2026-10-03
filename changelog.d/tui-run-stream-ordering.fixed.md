- **The interactive run stream preserves evidence of incomplete delivery.**
  Non-UTF-8 frames stop the child instead of being repaired. The workspace
  bounds event deduplication and marks late or out-of-order frames incomplete;
  a settled run alone does not establish a complete stream or verified Proof.
