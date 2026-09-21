- **The outcome states a trigger requirement beside the candidate (nika#1720).**
  A request that opens with a cadence ("every morning", "tous les matins",
  "at 9:00") or an outside event ("when Stripe sends payment_succeeded", "dès
  qu'un ticket arrive") compiled to a program that ran once per invocation and
  said nothing about the clause. `CompileOutcome.requested_trigger` (wire:
  `requested_trigger`, nullable, `compile_version` unchanged) now carries
  `{kind: schedule|webhook|event, source_hint, event_hint, payload_input,
  status: requires_binding}`; the program bytes stay trigger-agnostic (no
  cadence, hook id or secret), binding is the operator's or the product's
  gesture through the schedule contract, and the obligation ledger records the
  clause as a `trigger` duty realized by that requirement. A distributive
  trigger ("for each row") and a sequencing head ("once all three are done")
  state no requirement: the structure carries them.
