- **A fan-out's item table now tells a cancelled item from one that never
  started.** When `fail_fast` or a `timeout:` stopped a `for_each` batch, every
  item without a recorded outcome read `never_started`, including items that had
  already begun and sent their requests. A started item that is abandoned
  without a recorded outcome now reads `cancelled`. Only an item that never
  began keeps `never_started`. Recorded outcomes, outputs and the immediate
  `fail_fast` stop are unchanged, and the remaining items are never drained.
  Neither word says whether a provider billed a request (the ledger does). Paged
  tables gain an `items_cancelled` count, always present (0 included). Readers
  accept a paged table without that count only when it has no cancelled rows.
  An unknown status or a count mismatch leaves a paged table incomplete. This
  extends a closed trace vocabulary (spec 03/17), so it ships in the next MINOR
  after 0.121.
