- **A killed paid Run no longer blocks later unknown-cost Runs without a
  name.** A paid unknown-cost Run killed mid-dispatch (SIGKILL, a second
  signal, a host timeout), or one whose composition failed after its review,
  left its cost row `prepared`, and every later unknown-cost Run was refused
  with a message that named no Run and pointed at a gesture that did not
  exist. The Run now holds a writer lease beside its journal from before
  `prepared` until after `settled`, so the next review can tell a dead
  writer from a live one: it appends one `unknown` row naming the
  interrupted Run and keeps refusing until that exposure is reconciled
  (`nika trace cost reconcile`), and a live writer is refused as busy. A Run
  that ends without finishing settles what its account observed. Rows are
  appended, never rewritten, and nothing is retried. Separately, an
  unknown-cost Run whose `nika:write` declares `create_dirs: true` into a
  missing parent is no longer refused before its review with a bare
  « No such file or directory »; without the flag, the refusal names the
  parent and the flag.
