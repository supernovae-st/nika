- **A renamed column no longer loses the answer round.** « keep only the
  rows whose state is open » asked which observed field `state` means; the
  file then renamed `status` to `state`, and the answer `status` came back
  as « No current question owns this answer »: Session dropped the round.
  Now the answer is found stale and asked again over the file's current
  columns on the same request, and only a fresh explicit answer builds the
  proposal. Session and `nika compile --answer` carry the refreshed
  question forward; a plan recorded by an earlier version says so and is
  asked again once, never looping.
  **A number written with an exponent is a number again.** « 1.5e2 »,
  « 1E+3 » or « -1e3 » in a column a rule compares or ranks were read as
  text, so the compile asked what to do with them, and either answer then
  lost valid rows or stopped a valid run. The one number law, in the
  observer and in the written workflow alike, now reads the JSON number
  grammar with its exponent; a text whose value overflows (« 1e999 »)
  is still no number, and a plus sign in front, a bare or trailing point,
  `NaN` and `Infinity` stay text.
  **A budget stated with the work is the ceiling, never part of the
  work.** With a model chosen, « … write them to ./open.csv. Budget: $0. »
  was refused as « no further cognition admitted »: the budget read as an
  unfinished requirement only a model could build, and the zero ceiling
  forbade the model. « …, budget=0 » even added a filter on a column
  named `budget`. Session now reads the ceiling (« Budget: $0 »,
  « budget=0 », « --max-cost-usd 0 », « with a budget of 0 USD »,
  « plafond de 0 dollars ») and the compiler builds the rest of the
  request without it: the proposal or the field question comes back with
  no model call. A field named `budget`, a price, a quoted value or a
  path stay data and are no longer refused as a malformed ceiling; a
  malformed, negative, non-finite or conflicting ceiling still refuses
  before anything happens. « budget=0 » over a file that has a `budget`
  column reads both ways: nothing is built, and the reply says why
  instead of the bare « no further cognition admitted ». The same holds
  on a model reached through a gateway whose cost is unknown: the $0
  request was refused before its deterministic reading (« unknown-cost
  admission requires an exact HTTPS route », « the explicit zero
  constraint forbids this call »); it is now proposed with no call.
  **An incomplete `nika compile` says the file it left in place.** A
  compile that asks a question writes nothing, but the destination it
  was given may still hold an earlier file: `--json` now adds
  `existing_destination` and the text ends « existing destination
  remains at <path>; this compile did not write or remove it ». The file
  is not read, followed, removed or rewritten, `--force` included.
  Integration preserves compact trailing currency ceilings, validates malformed
  attached currency before filename exclusions, and reads joined ceilings and
  explicit old-default references together. Bare currency needs a separate
  segment; an `and`/`et` business range never loses its upper bound to a ceiling.
