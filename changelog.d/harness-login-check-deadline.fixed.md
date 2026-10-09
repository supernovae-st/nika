- **A slow login check no longer reads a signed-in seat as signed out.** A
  harness seat's own login command now has 30 s to answer: valid checks
  were measured at 12 s and 25.8 s, and the former 10 s turned them into
  a "not signed in" seat. The harness probe facts now also carry the
  check's answer (`login`): signed in, signed out, or unknown when it did
  not answer in time ("the login check did not answer within 30 s") or
  could not run, so an unanswered check is no longer the same fact as a
  signed-out seat. Admission still requires an answer before it admits a
  seat, and no credential is read.
