- **A project without `nika.yaml` is pointed at plain `nika init`.** `nika arm`, `nika arm fire` and
  `nika serve` answered « fix: `nika init --project-file` lays a commented starter », naming a hidden
  flag and leaving out what init writes. Plain `nika init` has laid that starter by default since
  #1283 (the flag only keeps older scripts working), among the project's starter files (agent
  briefs, hooks, editor and MCP wiring, a first workflow). The hint now says « fix: `nika init`
  writes the project's starter files, a commented `nika.yaml` among them (existing files are
  skipped) ».
