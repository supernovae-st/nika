- **The consent card names every file the run may touch.** When a workflow computes its paths
  (one file per item, a path built from a list), the card showed only the literal paths, such
  as « writes ./journal.md » for a run that also wrote ./traites/** and removed ./inbox/*. It
  now shows the grant in force. It also says « removes files » when the workflow removes any.
