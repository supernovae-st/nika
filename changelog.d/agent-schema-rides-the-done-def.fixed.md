- **An `agent:` task's `schema:` reaches the seat on the first request, and
  a `nika:done` result that misses it is repaired instead of fatal.** The
  loop-owned `nika:done` definition now carries the declared schema on its
  `result` parameter, the one binding every wire sends verbatim (including
  the seats whose API has no structured-output mode), so the model reads
  the exact shape instead of guessing it from the prompt. A `result` that
  fails validation goes back as that call's error observation and the
  model finishes again, drawing on the same `DEFAULT_SCHEMA_RETRY_BUDGET`
  the free-text re-ask uses, and the NIKA-464 verdict now says how many
  repairs were tried. Previously the first request carried no schema, and
  a non-conforming `nika:done` result died at once while a prose answer
  got two repairs.
