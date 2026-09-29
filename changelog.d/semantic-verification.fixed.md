- **A computation the authoring model writes now holds where no row is kept.**
  - « Sum qty over the rows where status is shipped » could compile READY to a program that
    was right on the rows it was shown but returned null when no row was shipped, so the run
    failed on its write.
  - Such a program is now also run on a source with no row and on each one-row source built
    from the model's own example rows: a program returning null on any of them is refused.
  - When the clause starts with a sum or a count word and the model's example returns a
    number (or one field holding one), the program must also return exactly 0 on the source
    with no row and a number on each one-row source; an error there is refused. This covers
    that phrasing and shape only, not every sum.
  - A refused program is sent back to the model once, with its program and the defect, within
    `--authoring-repairs` for the whole request. With no repair allowed, or when the call
    limit refuses the repair call before sending it, the request stays incomplete instead of
    READY. A program regenerated after a field answer is held to the same checks and repair.
  - An average, a minimum or a maximum of no row may stop the run with a stated error, and
    rows of no row are an empty list.
  - The checks read values on the model's own example rows, not on every source: they do not
    prove that the program keeps the right rows or returns the shape the request asks for.
