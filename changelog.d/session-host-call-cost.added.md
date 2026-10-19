- **The Session host's activity frames say what each finished call cost.** An activity frame's
  `call` now carries `cost` once the call returned, as the Session settled it:
  `{"kind": "priced", "nano_usd": "120000000"}` (nano-USD at list price, as a decimal string: an
  estimate, never an invoice) or `{"kind": "unpriced"}`. It is absent while the call runs, when
  nothing settled, and when its producer states nothing. A host can sum the priced calls for a
  live preparation cost, as the terminal does. The published contract now describes the frame's
  `call` (`SessionCall`) and its `cost` (`SessionCallCost`).
