- **`nika doctor` no longer calls a seat whose login check did not answer
  "not signed in".** A harness seat whose own login command did not answer
  within its deadline is now a warning that says the sign-in is unknown
  and why ("the login check did not answer within 30 s"), and it teaches no
  sign-in. A seat whose login command answered "signed out" still reads
  "not signed in" with its sign-in gesture.
