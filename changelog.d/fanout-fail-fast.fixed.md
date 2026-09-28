- **A `fail_fast` fan-out now stops at the first failure to complete, not
  the first in input order.** With `max_parallel: 2`, a held first item and a
  second item failing after 70 ms used to run on until the first item's
  `timeout:` (20 s), then record the held item as the failure and the real
  one as `cancelled`, its error lost. The fan now stops the moment any
  iteration fails. Items still in flight are dropped (`cancelled`), queued
  items never start (`never_started`), and every iteration that completed
  before the stop keeps its own row: a failure stays `failed`, and a success
  stays `ok` even when an earlier item was slower. Successful outputs, item
  rows and spend still read in input order. `fail_fast: false`, recovered
  items, `max_parallel`, per-iteration `timeout:` and the paged item table
  are unchanged. A dropped request still counts as sent, with an unknown
  cost, never a refund. Operator cancellation (Ctrl-C) is a separate
  contract that this fix does not change.
