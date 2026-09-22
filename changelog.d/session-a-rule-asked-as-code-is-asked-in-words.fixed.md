- **A rule the compiler can only ask as code is asked in words, never as
  code.** « Read ./sales.csv, compute the total and write it to
  ./total.txt » reached the human as « Which jq expression implements
  `the total` over the input object … » (the morning audit's case B:
  accepted unvalidated, then a run failure at `compute`). The session now
  asks the clause in words (« One thing I need from you, in words: how to
  do « the total » … your words take the place of « the total » in your
  request and Nika reads it again. No code is needed »); the answer
  replaces the quoted clause in the request and the compiler reads it
  again (« the total of the amount column » → Ready). A clause that stays
  code after the words, or one the request does not carry as quoted, is an
  honest incomplete that names the way on (« it would need a rule I can
  only write as code, and I never ask you for code »); the round is
  dropped, nothing written. `AuthoringRound` gains `restatements` (bounded
  to one).
