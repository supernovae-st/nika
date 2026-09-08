- **Keep session diagnostics free of payloads and refuse FIFO histories.**
  Persist only an outcome category beside the redacted dialogue; escaped Debug
  text could otherwise retain a value masked in the raw reply. Contained file
  opens reject a FIFO without waiting for a peer, so malformed history fails
  visibly instead of freezing the terminal.
