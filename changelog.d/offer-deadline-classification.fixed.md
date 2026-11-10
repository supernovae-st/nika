- **A silent adapter reads as unavailable whichever timer is observed
  first.** A session offer (`GET /v1/access-offers/{name}`,
  `nika_harness::session_offer`) is bounded by its own deadline, and the
  transport's idle bound arms on the same allowance a little later. When the
  process was polled late, the idle bound could be observed first: the row
  then carried `NIKA-1804` (session death) instead of `NIKA-1803` (adapter
  silent), and one harness test failed on a loaded runner. An error returned
  at or after the reading's deadline now ends as that deadline. An offer that
  was read is kept, and an error returned in time is still the adapter's own
  answer.
