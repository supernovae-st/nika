- **A lookup by identifier never picks a record by input order.** « Look up
  ticket 42 in ./tickets.json and write it to ./ticket-42.json » (or « find
  ticket 42 » with a model's help) used to write whichever record with id 42
  came first: two different records gave A in one order and B in the other,
  and the run succeeded both times. The workflow now resolves exactly one
  record: when several different records carry the identifier (including
  the text `"42"` beside the number `42`), the run stops before writing or
  posting anything and says how many records matched. Exact copies of one
  record still count as that record, no match still stops the run, and a
  duplicate added to the file after the compile is caught at run time too.
