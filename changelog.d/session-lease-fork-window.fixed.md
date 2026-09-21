- **A lease held for a fork window is not a foreign owner.** The
  conversation history's lease is a BSD flock; a sibling thread forking a
  run duplicates the descriptor until the child's exec closes it, so a
  second opener a few milliseconds early was refused as if another human
  held the project. `History::open` now waits a bounded grace (250 ms in
  5 ms steps) on `WouldBlock`; a lease still held past it is refused with
  the same message.
