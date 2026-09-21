- **A lookup by identifier binds its JSON file and selects the one record.** A
  lookup detail that names one JSON file and an identifier token (digits
  beside letters, `-`, `_` or `#`, or an email; never a bare number, a date, a
  path or a URL) binds the file without a directory question, keeps the
  identifier as `const.<slug>_id` (`ticket T-4471 in ./data/tickets.json` →
  `const.ticket_id`), asks only which field holds it (`const.<slug>_id_field`,
  a stable question), and selects the record with a jq that reads an array
  directory by field and an object directory by key. A literal lookup is the
  corpus: no `inputs.item` and no `inputs.record_id` is declared, and the
  classification, the draft and every endpoint payload see the record alone,
  never the whole file. The revision recheck reuses the same selector.
