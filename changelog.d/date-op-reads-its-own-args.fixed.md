- **`nika check` refuses a `nika:date` arg its op never reads.** `{ op: now, format: "%Y-%m-%d" }`
  passed the check, because `format` is an arg of `nika:date`, and ran without it: the file got
  `2026-10-15T21:39:15.511348+04:00`, matching neither declared format. Each op now reads only its
  own args (`now`: tz · `add`/`subtract`: base, duration, tz · `format`: input, format, tz ·
  `parse`: input, format · `diff`: start, end, unit), and an arg of another op is refused as « not
  an arg of `nika:date` op `now` » with that op's own args. A templated op is left to the run.
