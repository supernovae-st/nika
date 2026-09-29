- **A workflow the authoring model shapes is READY only once it is checked against the whole request.**
  - A candidate compiled from the model's plan could be READY while it missed, reordered or
    changed part of the request, because only the duties the compiler extracted, or the
    words a step restated, were checked. Every clause no deterministic law reads from the
    workflow's bytes, and the whole request, is now judged against the workflow's own bytes
    by a bounded judge that reads the full request, its answers and the observed files; task
    names, labels and the model's confidence count for nothing.
  - A part the judge finds missing is sent back to the model with the judge's findings,
    within `--authoring-repairs`, and a computation the repaired plan still needs is
    regenerated with them. A judge that abstains or fails, or repairs that do not settle it,
    leave the request incomplete, naming the part; no question asks for what the request
    already states.
  - The judge's calls are authoring calls: counted in the receipt with their usage, and held
    to `--authoring-max-calls`.
  - A saved plan replayed on an answer round keeps what the compiler checks by itself, with
    no call; what only a judge can settle is judged in that round, or stays incomplete
    without a judge. Nothing in a saved plan or an answer is read as a judgment.
  - A computation the authoring model writes now holds where no row is kept: it is run on a
    source with no row and on each one-row source built from the model's own example rows,
    and a program returning null there is refused. When the clause starts with a sum or a
    count word and the model's example returns a number (or one field holding one), the
    program must also return exactly 0 on the source with no row and a number on each one-row
    source; an error there is refused. This covers that phrasing and shape only, not every
    sum. An average, a minimum or a maximum of no row may stop with a stated error, and rows
    of no row are an empty list.
  - A refused program goes back to the model once, with its program and the defect, within
    `--authoring-repairs` for the whole request; with no repair allowed, or when the call
    limit refuses the repair before sending it, the request stays incomplete. A program
    regenerated after a field answer is held to the same checks and repair.
  - Limits: the judge is a model, so its approval is bounded evidence, not proof. The empty
    checks read values on the model's own example rows, not on every source.
