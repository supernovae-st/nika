- **The seat repairs an evidence it misspelled with one bounded call.** When a COLD
  proposal cites an evidence the request never wrote (a model answers `extraits` for a
  request that wrote `extrais`), the compiler sends the seat's own answer back with the
  verifier's counterexample, once, under the same output cap and timeout; the repaired
  proposal is judged by the same merge, a second miss is refused as before, and the receipt
  counts both calls (`provenance.authoring.calls`, the route `cold: repair N`, the sample
  record's `calls`). Measured on eco-60 with gpt-5-mini before the change: 4 of 52
  proposals were refused on that defect alone.
