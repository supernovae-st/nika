- **One edit call, several passages.** `candidate_edit` takes `edits`, a list of old and
  new passages applied as one revision: each old passage is found exactly once in the
  candidate and none overlaps another, so an author repairs several places in one call.
