- **`nika --tui` never leaves a dead door: a terminal the renderer cannot
  take gets the plain session.** The renderer's entry is now two steps
  (`app::enter` takes the terminal · `app::run_on` drives it), and the CLI
  decides what a refusal becomes: `TERM=dumb`, or a terminal that never
  answers the cursor-position report the inline viewport anchors on
  (bounded by crossterm's wait), is said once on stderr — « the renderer
  cannot take this terminal (…) · the plain session opens instead » — and
  the same session opens as the plain loop, with everything the renderer
  had enabled restored first. The terminal matrix is proven on a PTY: the
  renderer opens, helps and closes at 60×20, 80×24 and 120×40; a resize
  while a proposal waits re-anchors the viewport and redraws the consent
  prompt; `TERM=dumb` writes no cursor query and no CSI sequence; a mute
  cursor report falls back within the bounded wait.
