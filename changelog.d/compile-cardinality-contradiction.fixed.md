- **Contradictory bounds on the produced content are refused, never run.** A
  request asking for a report of "exactly 5 lines" that is also "at least 12
  lines long" compiled READY and ran on a prompt that silently obeyed one of
  the two. The bounds a request states on one unit of its content (exact,
  minimum, maximum, in six languages) are now read as intervals; two that
  cannot meet contradict each other, the outcome is refused with both bounds
  named, and the obligation ledger records both duties as contradicted.
