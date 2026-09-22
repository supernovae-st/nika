- **An interruption is heard while the renderer runs a turn.** A seat
  call that never returned held `nika --tui` until it did: Ctrl+C and
  SIGTERM were queued behind the turn. The turn now runs on a detached
  worker thread (the conversation travels with it and comes back with
  the result) while the shell keeps reading the terminal: one Ctrl+C
  warns in the busy row (« interrupted: the call cannot be recalled ·
  Ctrl+C again leaves now »), a second one or SIGTERM leaves at once
  with the terminal restored and the call left to die with the process;
  keys typed ahead wait for the turn. The busy row counts the seconds
  (« working through your words · 12s · Ctrl+C twice leaves »). The
  plain session keeps the line discipline's own Ctrl+C.
