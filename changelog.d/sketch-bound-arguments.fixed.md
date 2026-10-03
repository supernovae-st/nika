- **A sketched tool can no longer write or read another task's file.** When a workflow is
  composed from a sketch, every file argument a builtin takes (a chart's output or data file, an
  image's input, output directory, references or mask, a speech output directory, a decision
  bundle, a multipart upload) must be one of the paths its own task states, on the side it reads
  or writes. Another task's file is refused before a candidate exists, even when the workflow's
  permissions would allow it; a templated or malformed path is refused instead of ignored. A
  task that never states the file its tool always reads or writes (an edit stated only as a
  write, a chart with no output) is refused while the sketch can still be repaired, not after. A
  refused fill names unknown tasks, fields and keys by their position only, so text a model
  proposed is never repeated back.
