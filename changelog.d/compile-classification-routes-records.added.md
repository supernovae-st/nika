- **A classification of each record routes the records.** « Read
  ./tickets.json, classify each ticket as bug or feature, and write the bugs
  to ./bugs.json and the features to ./features.json » runs the classify per
  parsed record with one record in its prompt, and each write carries the
  records routed to the category its clause names (a jq over records and
  categories, never the bare category word); « write the results to
  ./out.json » carries every record with its category. One write naming two
  files (« write them to ./bugs.json and ./features.json ») is one write per
  destination, the later path sharing the earlier object, the category read
  from the clause's prose with the paths stripped, then from the file's own
  stem. A draft asked as bullets or points is laid out one per line.
