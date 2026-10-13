- **A run now settles a crashed run's leftovers when it starts.** A run killed before its
  end (SIGKILL, a machine that went down) left the files it created in place as if
  whole, and the original bytes of the files it changed in `.nika/originals/`, where
  nothing ever mentioned them. Each file a run first writes now leaves a small record
  there, with the digest of a created file's bytes after each write. The next `nika run`
  in the project settles a run whose journal shows it dead: a file it created moves to
  `.nika/quarantine/<its run>/` only while its bytes are still the ones it wrote (a file
  you changed since stays where it is), a file of yours it changed stays and its
  original moves to `originals/`, and a `manifest.json` lets `--resume` of that run put
  the moved files back. A run that is still alive, or whose state cannot be proven, is
  never touched. Each action is one line on stderr.
