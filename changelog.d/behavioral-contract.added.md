- **A request's results now have a behavioural contract a rehearsal can be judged
  against.** From its reading of the request alone, never from a candidate, the
  compiler states which files the request wants written and what they must hold
  (filters, a sort with its tie rule, the first N rows, groups and totals,
  projections, duplicates), and judges what a rehearsal consumed and wrote by
  value: exact decimal numbers, `70` equal to `70.0`, rows as a multiset or in
  the stated order, any tied row at a cut unless file order is stated. Nothing
  passes on what the reading cannot prove: a write it cannot prove
  unconditional, an output name it cannot trace to the request, a sample, a
  truncated read, an unsupported operation or a case the request leaves open is
  reported unverified. A failed run never passes by what it did not write, an
  engine failure is a defect, a stop is never counted as the requested end (an
  error message never stands for one), a stop never hides a wrong value already
  written, and an invalid fixture or observation is reported as such, never as
  a verdict on the workflow. A request of the small closed form (read one
  file, keep or count the rows where a field is a value, write the result to
  one file) proves its write required and its count's name free or stated, so
  a right result can be certified. The engine and Session APIs now build two
  checked forms of the closed `copy SOURCE as is to TARGET` request and select
  by rehearsed output before consent. This path has been exercised on
  synthetic files; the other operations remain library contracts.
