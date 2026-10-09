- **A harness authoring call is no longer cut while its agent is visibly
  working.** An ACP authoring call's per-call time (600 s by default) used
  to bound its whole duration, both in the harness and around every
  compile authoring call, so a call still streaming its reasoning near
  that limit was timed out with no answer. It now bounds the call's
  silence: armed when the call starts, re-armed by each answer, thought,
  tool or plan update of the call's own session, so the call times out
  only once a whole allowance passes with no such frame, and its
  transport is given that same allowance per frame. The compile layer no
  longer wraps a harness route in a total of its own; API routes keep
  theirs. Usage and status updates, another session's or call's frames
  and stray lines never re-arm it; Stop still ends the call at once,
  nothing is retried, and the 408 timeout form is unchanged. The
  terminal record's `bounds` now say `deadline_ms: null`, the allowance
  as `idle_ms` and when an activity frame last re-armed it as
  `rearmed_ms` (null when none did). A direct (native) harness seat
  shows no frames and keeps its deadline.
