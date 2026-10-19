- **The Session no longer crashes when the decision seat skips an oversized question.** After the
  seat had refused one question alone for its context, a later, larger question was skipped
  without a place in the request journal. The next request then settled at the wrong index and
  the terminal app panicked (`batch.rs:121`). Every skipped question is now journaled as never
  sent, in its own place.
