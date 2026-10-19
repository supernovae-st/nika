- **Two projects that arm a beat of the same name keep their own OS units.**
  `nika arm --emit … --write` named a unit after the beat alone
  (`nika.arm.daily`). A second project's `--write` replaced the first
  project's unit, `nika arm disarm daily --write` in one project deleted the
  other's, and a flagless `--write` could copy another project's env-file
  wrap, so its runs sourced that project's keys. Unit names now carry the
  project's key (`nika.arm.<key>.daily`, the start of the project root's
  fingerprint), and neither the emit nor disarm reads, replaces or removes
  another project's unit. Disarm now prints `launchctl bootout
  gui/$UID/<label>`: the printed path form failed once the plist was gone, so
  the job stayed loaded until logout. Units an older engine wrote are
  reported with the commands that retire them; re-run `nika arm --emit
  <target> --write` in each project. A project whose `.nika/arm/env-file`
  already names another project's file keeps it until you pass `--env-file`.
