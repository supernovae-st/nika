- **A second Ctrl-C aborts `nika run --output json` and `nika test` at
  once.** Their fold held the process-wide stderr lock until the run
  ended, so every other writer waited for it. The signal thread prints
  its notice before it exits, so the abort waited for in-flight work to
  finish, and the egress journal stalled confined connects the same way.
  The fold now takes the lock one write at a time, like the default lane.
