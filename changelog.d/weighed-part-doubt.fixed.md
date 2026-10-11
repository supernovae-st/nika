- **A part judged missing on an unsure answer no longer refuses a
  workflow.** On identical bytes, a decision model's answer on one part
  of the request moved across its old cut, so the same workflow was
  refused on one run and proposed on the next. A part's weighed
  « missing » now decides only at the confidence a whole-request
  rejection needs (0.75), read over that part's own options; below it,
  the part is stated with the proposal and nothing more is asked of it.
  At or above it, a part the model ties to a step goes back to its
  author as before, and one it ties to no step is stated, never held.
  The same holds over a trial run: a part the question over the run
  names below that confidence is stated, never traced to a step, and a
  part left open stays open. A judge that reports no distribution keeps
  its verdict.
