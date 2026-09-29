- **A count stated over a filter now counts.**
  - « Count the rows where status is paid » used to compile to a workflow that wrote the paid
    rows instead of their number, while its checks and runs succeeded. It now writes
    `{"count": n}`: 0 when no row matches, with several conditions, with a numeric threshold,
    and with the rows' own noun (« count the orders where … »). « The number of rows whose … »
    and « compte les lignes dont … » count too.
  - Words before a filter that the compiler cannot account for (another operation such as a
    sort, or a qualifier like « the paid rows where … ») are no longer dropped: that clause is
    left to the authoring model instead of compiling a narrower filter.
  - A plan saved with the old reading is refused when it is replayed; compiling the request
    again gives the counting workflow.
- **Operations run in the order the request states.**
  - « Keep the 2 rows with the highest amount, then keep the rows where status is paid »
    used to filter first and rank second, so it could write two paid rows the request never
    kept. It now ranks, then filters the two kept rows; « keep the paid rows, then the 2
    with the highest amount » still filters, then ranks, and « count them » after a top-N
    counts the kept rows.
  - A filter stated after a sort or a total now runs after it: a malformed number that the
    sort or the total reads stops the run, as the request's order implies, instead of being
    filtered out first while the run writes.
  - A step the compiler cannot run in the stated order (a filter on a column after a total,
    or after a grouping on another column) is left to the authoring model instead of being
    run in another order.
  - Plans saved before this change keep their bytes; an older binary refuses a plan that
    records ordered steps.
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
- **A filter the compiler cannot read is never taken for a description of the data.**
  - « Keep the rows whose status is a » compiled READY into a workflow that wrote every row:
    the value `a` kept the compiler from reading the condition, and the clause was taken as
    a sentence about the file. It is now reported as work the compiler cannot carry and no
    workflow is produced; with a value it reads, the same request still filters.
  - With an authoring model configured, that clause goes to the model instead of a question: the
    model's filter over the request's own value runs beside the sort the compiler reads.
