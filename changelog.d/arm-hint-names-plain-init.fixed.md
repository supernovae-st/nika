- **A project without `nika.yaml` is pointed at plain `nika init`.** `nika arm`, `nika arm fire` and
  `nika serve` answered « fix: `nika init --project-file` lays a commented starter », naming a hidden
  flag. Plain `nika init` has laid that starter by default since #1283 (the flag only keeps older
  scripts working), so the hint now says « fix: `nika init` lays a commented starter (existing files
  are skipped) ».
