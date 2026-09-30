- **Refuse a candidate whose jq reads a document key on one of its records.**
  A seat's `nika:jq` expression that reads a key of its input document inside the iteration over
  that document's records, such as `.window_start` inside `.records[]`, reads null on a record:
  a window compared against it keeps nothing and the total is silently 0, or a conversion of
  the null fails at Run. When the host observed the records' file and its columns do not carry
  the key, the candidate is now refused (`RECORD SCOPE`) with the file, its columns and the
  repair: bind the document before the iteration (`. as $doc`) and read `$doc.window_start`.
  A column of the same name, a variable bound before the iteration and a read at document
  level stay admitted, and nothing is judged without an observation of the records.
