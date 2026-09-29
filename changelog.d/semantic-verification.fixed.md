- **A computation the authoring model writes now holds where no row is kept.**
  - « Sum qty over the rows where status is shipped » could compile READY to a program that
    was right on the rows it was shown but returned null when no row was shipped, so the run
    failed on its write. A sum or a count is now checked on a source with no row and on each
    one-row source built from the model's own example: it must return 0 there.
  - A program that fails this check is sent back to the model once, with its program and the
    defect, within `--authoring-repairs` for the whole request; with no repair allowed, the
    request stays incomplete instead of READY. A program regenerated after a field answer is
    held to the same check and repair.
  - An average, a minimum or a maximum of no row may stop the run with a stated error, and
    rows of no row are an empty list; a program returning null there is refused the same way.
  - The check reads values on the model's own example rows, not on every source: it does not
    prove that the program keeps the right rows or returns the shape the request asks for.
