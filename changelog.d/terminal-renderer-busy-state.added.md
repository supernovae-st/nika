- **The renderer shows work is active before a turn runs (`nika --tui`).**
  A turn is synchronous, so the busy state is drawn before it starts, with
  the conversation's own name for the work (« working through your words »,
  « landing the exact bytes and checking them », « answering the gate »,
  « seating the intelligence you chose »), and the turn's first word clears
  it; a slash command draws none. `/help` is answered by the engine inside
  the viewport, proven on the real binary through a PTY.
