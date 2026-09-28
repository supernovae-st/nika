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
