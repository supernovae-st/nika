- **File-protection clauses no longer request an extra read during rehearsal.**
  When an intention reads a file and says not to modify its parent directory,
  the closed English and French protection forms leave that parent out of the
  requested inputs and outputs. Paths match whole literals, so a child path
  cannot give its parent a read role. Independent and shared reads remain
  inputs, and conditional or unrecognized clauses are not discarded. An
  explicit directory read still cannot supply a file's rehearsal witness.
