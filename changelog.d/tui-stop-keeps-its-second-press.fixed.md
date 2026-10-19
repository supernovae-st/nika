- **A second `Ctrl+C` after a stop now leaves, even once the stopped turn has
  ended.** While Nika prepares, the first `Ctrl+C` asks the preparation to stop,
  and the screen says that a second press leaves. A stop ends a call under way
  at once, so the turn often ended before the second press was read. That
  press then only armed the idle prompt, and the door stayed open. Now the idle
  prompt keeps the first press armed (« Ctrl+C again leaves · any key stays »).
  The next `Ctrl+C` leaves, and any other key keeps the session. A correction
  queued during the turn is still sent as the next line, with no press armed.
