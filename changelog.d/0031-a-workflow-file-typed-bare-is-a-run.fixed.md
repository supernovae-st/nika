- **A workflow file typed bare is a run.** `nika notes.nika.yaml` used to
  answer `unrecognized subcommand` — a wall where a run was meant. A
  colleague sends a `.nika.yaml` and the person types its name the way one
  opens a file. The routing is narrow on purpose: the name must end in the
  workflow suffix AND be a file on disk, so a typo'd verb keeps clap's
  did-you-mean and a suffix naming nothing keeps clap's error.
