- **A Unicode regex class is found before the run.** `test("\\p{L}")` compiles as jq, then the
  runtime's regex engine refuses it on the first item: a clean check was followed by a failed
  run. The check now names the class and the way out (an explicit set such as
  `[A-Za-zÀ-ÖØ-öø-ÿ]`), so the author repairs it before proposing.
