- **A decision model's unsure verdict no longer holds a workflow.**
  Checked against workflows whose correct result is known, a decision
  model's answer on the request as a whole was reliable only as a
  confident rejection: its « faithful » never reached a confidence of
  0.6 and was wrong 5 times in 13, and its « unfaithful » below 0.75
  also held correct workflows. Now a weighed verdict decides only as a
  rejection at that confidence or more; otherwise the parts of the
  request, each judged alone, and the extra-task question decide. A
  workflow they carry is proposed with the doubt stated in words, never
  held or turned into a question about the verifier; a part judged
  missing goes back to its author once, and is stated if judged missing
  again. A judge that reports no distribution keeps its verdict.
