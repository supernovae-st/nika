- **`nika:convert` reads semicolon and tab separated CSV, and writes a null as an empty
  cell.** A European spreadsheet export (`nom;montant`, decimals with a comma) was read
  with the comma: it became one column, or failed on its first decimal. The separator
  is now counted on the first line outside quotes: a semicolon or a tab that outnumbers
  the comma is the separator, otherwise the comma stays (RFC 4180). JSON to CSV wrote a
  `null` as the text `null`; it is now an empty cell, as a missing key already was.
  Cells are still strings: convert numbers with `tonumber` before arithmetic or
  ordering.
