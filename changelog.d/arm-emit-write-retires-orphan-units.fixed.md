- **`nika arm --emit --write` retires the units of beats `nika.yaml` no longer arms.** A beat
  removed from the registry, renamed, or moved to the cloud kept its OS unit, which went on
  waking `nika arm fire` for a beat that no longer exists; neither `--write` nor a dry
  emit mentioned it. Now `--write` syncs the units with the registry: each of this
  project's units whose beat is no longer armed is named with the command that unloads it,
  then its files are removed. Loading and unloading stay your gesture, as before. A dry
  `nika arm --emit` lists what `--write` would retire. Another project's unit, and a unit
  without this project's GENERATED mark, are never touched.
