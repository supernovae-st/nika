- **The decision seat's journal says how long each request took.** Every
  System One request that left, a single question or one request of a
  batch, now records `elapsed_ms` in the Session's decision-seat
  observation (`nika/session-decision-seat@2`, additive): the time its
  transport took, measured by the host. A request never sent records none.
