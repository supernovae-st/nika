- **A workflow saved with a byte-order mark is framed on the right line
  (#1608).** The parser strips a leading UTF-8 byte-order mark, but every
  span then indexed the stripped text, 3 bytes left of the file `nika check`
  frames: an unknown key at column 1 was framed at the end of the line above
  it (`5:24` instead of `6:1`), and the `--json` spans, trace and debugger
  lines carried the same shift. Spans now index the text the caller passed
  in, byte-order mark included. The frame and the browser checker's `col`
  count line 1 from the character after the mark, as an editor shows it, so
  a finding on line 1 keeps its column (`1:1`, as without the mark). A file
  without one is unchanged.
