- **A Session's run runs only the world its check judged, children
  included.** A run the Session requested was bound to its workflow's
  own bytes, so a child workflow or a skill rewritten after the check
  still ran. The check now reads the workflow and every child it reaches
  once, with the run's own reader, and binds the request to that world's
  closure; the child run compares it after admission and refuses any
  other world before any task. A workflow the run cannot capture, such
  as a symlinked one, is no longer reported clean.
