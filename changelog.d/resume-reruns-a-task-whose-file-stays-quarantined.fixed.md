- **A resumed run no longer calls a task done when its file stayed in quarantine.** When
  `--resume` could not put a failed run's file back (something now stands at its path, for
  example), it said so on stderr, then still counted the task that wrote it as a cache hit
  and could finish `succeeded` while that task's bytes sat in `.nika/quarantine/<run>/`.
  Now that task runs again, with every task after it, and the run says which one. Like a
  fresh run, the re-run writes its file over whatever stands at that path; if the resumed
  run then succeeds, that earlier file is not kept. A file the resume did put back belongs
  to the resumed run: if that run fails too, the file moves to its quarantine again instead
  of passing for a whole one.
