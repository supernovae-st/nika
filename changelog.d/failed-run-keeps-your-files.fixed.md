- **A failed run no longer moves a person's own files into quarantine.** When a run
  failed, `nika run` moved every file a successful `nika:write` or `nika:edit` returned to
  `.nika/quarantine/<run>/`, including a `notes.md` that existed before the run and was
  only edited. The next run then failed to read or edit it. Now only files the run created
  move. A file that existed stays where it is, and its original bytes, copied before the
  run first changed it, wait under `.nika/quarantine/<run>/originals/`. A writer reached
  by `after: failure`, `terminal` or `unwind` keeps its file, since it records the failure.
  The quarantine dir carries a `manifest.json`, and resuming the failed run with
  `--resume` puts the moved files back before it starts. The run says on stderr which
  files moved and where each kept file's original is.
