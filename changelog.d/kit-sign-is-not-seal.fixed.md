- **The agent kit separates signing a workflow from sealing a run.** The operating skill's audit
  checklist asked for a run « signed (`nika sign`) », but `nika sign` binds a workflow file to its
  author, and only the run key `nika key init` mints seals a journal. The checklist now asks for
  both, each with its own command. The portable plugin manifest's description opens with the
  README's « Say it once. Keep it as a file. » instead of the retired intent-as-code pitch.
