- **The author waits for a fan-out and keeps independent items.** Its card showed `fail_fast: true`
  as the one form of a fan-out, and never said that a fan-out over an empty collection is skipped
  with a `null` output: an inbox workflow failed with NIKA-VAR-006 on an empty folder, its steady
  state, and one failed summary of two lost both. The card now shows the plain fan-out, has every
  task that reads a fan-out's output wait for it with `after: { <id>: success }` (an empty
  collection then ends the run with nothing to do), and, when each item stands alone and the
  request does not want all or nothing, keeps the other items with `on_error: { skip: true }`.
