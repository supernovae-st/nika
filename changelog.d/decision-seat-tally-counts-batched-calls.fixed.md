- **The decision seat's tally counts every call it sent.** A batched decision call (the whole
  request's verification, its parts, Foundry qualification) is answered as a whole, but the
  tally counted only single-question answers: `/status` said « 33 call(s) sent · 7 answered · 0
  without a response ». A batched call that came back now counts as answered, and one the seat
  answered with an HTTP error as without a response.
