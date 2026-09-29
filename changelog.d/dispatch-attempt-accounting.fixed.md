- **Every provider request a Run sends stays on its ledger, even when the task
  stops waiting for it.** When a `for_each` item failed, `fail_fast` aborted the
  siblings still in flight, and their requests, already sent, dropped out of
  the terminal `unpriced_calls` count. A `timeout:` did the same to the attempt
  it cut, and a provider's own 429 re-send could vanish with it. The count
  depended on which sibling happened to answer first: E17's three-item fan-out
  sent three requests, and the ledger said 1 on one build and 3 on another.
  Now each request is recorded when it is handed to the transport. A request
  whose task was cut is counted once: as an unknown charge if it never
  answered, never as a known zero, and by its own price if it had answered.
  `fail_fast` still aborts the remaining items immediately.
