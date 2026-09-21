- **A stated bound on the drafted text is verified at run, not only prompted.**
  "3 bullets", "12 lignes max", "under 150 words", "genau 5 Zeilen" reached the
  language step as prompt guidance and nothing checked the result. The bounds a
  request states on a measurable unit of the text (lines, bullets, words,
  sentences, characters, paragraphs; six languages; exact, minimum, maximum,
  strict) now lower to a jq law over the drafted body (`draft_bounds`) and an
  assert (`draft_bounds_admit`) the write waits for, and the obligation ledger
  records each such duty as realized by that law ("verified at run"). A bound
  on a unit nothing measures at run (a page, a token) stays prompt guidance and
  the ledger says so.
