- **One gesture: bare `nika` opens the renderer.** On a real terminal
  `nika` now opens the session behind the inline viewport, as the agent
  CLIs a human already knows do; the `--tui` flag is gone. The plain
  line loop stays one gesture away for flat text (`nika --plain`, or
  `NIKA_TUI=0` in the environment: a screen reader, a recorder, a
  harness), and it remains the automatic fallback when the renderer
  cannot take the terminal (`TERM=dumb`, a terminal that never answers
  the cursor report), said once on stderr. A pipe still gets the
  deterministic concierge.
