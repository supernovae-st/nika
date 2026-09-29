- **Keep the session renderer under `--ascii` and draw its own glyphs in ASCII.**
  Bare `nika --ascii` opened the plain line loop; it now opens the same terminal renderer as
  bare `nika`, and the renderer's own glyphs take their ASCII twins: its block faces, its loader,
  its live prompt marker, its hints, the focus rule and the title the door gives the terminal.
  The Session's own words (the banner, replies, the status line, the lifecycle rail) and the
  echo of a sent line are shown as written, so the screen is not pure ASCII. `--plain` and
  `NIKA_TUI=0` keep the plain loop.
