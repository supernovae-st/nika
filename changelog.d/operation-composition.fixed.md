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
