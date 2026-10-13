- **A failed run's journal records its quarantine, signed or not.** What the teardown did
  with a failed run's files rode only inside the signed `run_sealed` frame, so a run
  without a signing key journaled nothing about it, and a crash between the moves and the
  seal left only the manifest. A `quarantine_recorded` frame now lands right before the
  seal: the quarantine directory, how many created files moved (and any move that
  failed), how many of your files stayed in place, and the manifest with the sha256 of
  its bytes. A signed run's seal covers it like every earlier line.
