- **A rehearsal tries the day a gated step is skipped.** A workflow could pass its check and
  its rehearsal, then fail its first run with nothing new: a filter after a skipped step read its
  null (« cannot use null as string », « cannot use null as iterable »). After its run, a
  rehearsal now evaluates each such `nika:jq` step on that null, in its bounded jq process. It
  fails with `NIKA-REHEARSAL-SKIPPED` only when the null alone makes the filter fail: the same
  filter passes with a value in its place. The author then guards the step before anything is
  proposed.
