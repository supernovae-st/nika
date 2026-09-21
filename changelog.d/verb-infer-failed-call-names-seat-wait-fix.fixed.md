- **A failed provider call names the seat, the wait and the fix.** Under
  the one spec code `NIKA-INFER-001` the infer verb now reads a failed call
  in three shapes: the transport's bounded backoff spent on a rate-limited
  or overloaded seat (the seat, the round-trips and the wait, still
  transient for an authored `retry:`); a schema refused at the door while it
  travelled natively (the seat, the wire, the provider's own identifiers,
  and the fix: simplify the schema or seat a model the catalog lists with
  `json_mode: schema`; the reply was never sampled); everything else as
  before. On success the receipt carries the transport's account
  (attempts, waited, statuses), summed across schema-repair round-trips
  like usage is.
