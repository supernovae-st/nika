- **An OS unit left behind by an edited schedule is named and refused.** After a cadence
  change in `nika.yaml` without re-running `nika arm --emit … --write`, the installed unit
  kept waking `nika arm fire` at the old time; the beat answered « not due » every day and
  nothing woke it at the new time, while `nika arm` still reported it READY. Each beat
  unit now carries the revision of the schedule it was written for (the cadence, and for
  launchd the machine zone it was rendered in) and hands it to the firer, which refuses a
  wake from a stale unit with the re-emit that fixes it and fires nothing. `nika arm` lists
  such units under « unités périmées », and also names a unit written by an older engine,
  which carries no revision. Re-run the emit with `--write` once to give existing units
  their revision.
