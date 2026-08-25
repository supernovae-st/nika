- **`issue-proof` can now read an issue that talks about `issue-proof`.** The
  body parser took the first line *containing* `proven_by:` anywhere, so #1200
  — the issue about this very gate — could never be closed: its body quotes
  the old guard, `contains(github.event.issue.body, 'proven_by:')`, inside a
  code fence, and the parser extracted `)` as the job name and reopened the
  issue. Measured, not reasoned: `JOB=[)]`. An issue that discusses the proof
  mechanism is exactly the issue most likely to report a defect in it, so the
  parser has to be able to read its own subject matter. A trailer is now a
  line that is *only* the trailer, and the last one wins — which steps past a
  fenced quote, an inline backticked mention, and a prose line that happens to
  wrap onto column 0.
