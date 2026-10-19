- **A newline after a lone reference is written.** `content: "${{ with.d.version }}\n"` wrote
  `3.15.0` without its newline, and `"${{ x }}\n\n"` lost both: a string holding one reference
  and only whitespace around it was read as the reference alone, which keeps the value's type and
  drops the whitespace. Only the reference alone keeps its type now; any other character around
  it, whitespace included, is text. A `for_each` collection is still read whole when a block
  scalar leaves whitespace around its reference.
