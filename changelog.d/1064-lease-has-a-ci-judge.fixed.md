- **The pre-push lease now has a judge that runs.** #1064 shipped a lease
  that serialises the pre-push gate, with a self-test proving mutual
  exclusion, stale reclaim, and that a live owner is never robbed — and
  nothing ran it. Not CI, not even pre-push, because a hook helper has no
  runner. It was a green that had never been asked a question. The
  `issue-proof` gate, shipped in the same merge, reopened #1064 within
  minutes for exactly that: a close whose proof does not exist. The first
  thing it caught was its own author. `gate-lock` joins the ratchet matrix
  beside `credential-headers` and `changelog-fragments`, which are there for
  the same reason one degree less severe, and it fails closed when the
  self-test is missing rather than reading an absent file as a pass.
