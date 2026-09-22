- **Leaving is always one line away, and a stray `yes` is refused.** `/quit`
  at the consent prompt was answered « that line is not a consent » and the
  human could not leave without killing the process; it now drops the
  proposal (nothing written) and closes, as it does at the first screen
  (no choice made) and at a human gate (the gate keeps waiting in its
  trace and in the record). A bare `yes`, `ok`, `oui`, `no` or `non` with
  nothing pending was sent to the compiler as work (« I read this as work
  but cannot build it yet »); it is now refused as a wrong state — a
  proposal asks `apply? ›` first — and never reaches a model.
