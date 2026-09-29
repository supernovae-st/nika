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
