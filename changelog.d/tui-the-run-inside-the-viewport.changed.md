- **The run shows inside the renderer's viewport.** « run it » in
  `nika --tui` used to hand the terminal back: the plain run path
  printed below the viewport, then the viewport was re-taken (a cursor
  report asked again). The door now runs its own machine lane
  (`nika run --json`) as a child whose pipes never touch the terminal:
  each frame becomes one line of the run's story in the busy row
  (« → read_source · invoke · nika:read », « ✔ read_source · 3 ms ·
  1/2 », « ◇ paused · `approve` asks you »), the story is committed as
  one block when the run settles, then the result view. A human gate
  pauses headless and returns as the gate view with its own prompt; the
  answer resumes through the same lane. A run still in flight when the
  door leaves is ended with SIGTERM (the engine cancels, the trace says
  so), never orphaned. The plain session keeps its own run renderer.
