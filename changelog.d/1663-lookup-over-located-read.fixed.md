- **A lookup by identifier selects its record in the file the request already reads.**
  « Read ./tickets.json, find ticket 42 and write it to ./ticket-42.json », settled as a
  lookup by a decision seat, asked for a second « directory » (`const.ticket_42_directory`)
  although the read clause locates the material. The lookup now binds the read file, asks
  only which field holds the identifier (`const.ticket_id_field`), reads the file once and
  writes the selected record. In a lookup detail a bare number after a word (« ticket 42 »,
  « order 1002 ») is the identifier; everywhere else a bare number stays a count or a bound.
