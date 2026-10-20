- **`nika check` names a read that breaks when its producer is skipped.** A task
  that can be skipped (a `when:` that can close, an empty `for_each`, `on_error:
  skip`) hands its readers `null`, and a `with:` binding is evaluated before the
  reader's own `when:`: `u: ${{ tasks.x.output.url }}` guarded by
  `when: ${{ with.u != null }}` failed the run with `NIKA-VAR-001` while `check`
  said « run ready ✔ » with zero hints. `check` now carries a `skipped-read`
  hint for such a read, in a binding or in a `when:`, `for_each:` or verb field
  stepping into a whole bound value that no `when:` rules out as null. The hint
  names the reader, the read, the producer, why it can skip and the repair: bind
  the whole `tasks.x.output`, test it in `when:`, then step into it in the verb.
  Guards that hold once the producer skipped stay silent (`has()`, `!= null &&`,
  the producer's own consent guard repeated), so does a task the skip cancels
  or a verb read its `on_error:` recovers. Advisory: `is_clean` is unchanged,
  and past the 2,000-task analysis cap the miss is stated.
