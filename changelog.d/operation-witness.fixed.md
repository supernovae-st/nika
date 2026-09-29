- **Each operation the request states is checked against the program that runs.**
  - The compile record lists every filter condition, count, order and cut the request
    states, with the request's own words, its place in the stated order and the fields it
    reads. Each is marked done only when the compiled program does it with those
    parameters, never because a step is named after it.
  - A proposal that ran the paid filter before « keep the 2 rows with the highest amount »,
    or left the filter out, compiled READY and wrote a paid row outside the top 2, or an
    unpaid one. The reading of the request's own clauses now replaces such a proposal, and
    a program that misses a stated operation is never READY.
  - Words the compiler cannot read are listed as unverified, not as done.
  - An operation the request does not state is never run: a proposal that added the paid
    filter before the top 2 as well as after it ranked only the paid rows, READY. The reading of
    the request's own clauses replaces it.
