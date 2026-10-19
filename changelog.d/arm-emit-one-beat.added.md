- **`nika arm --emit <OS> --beat <label>` emits one schedule's unit.** With `--write`, only that
  beat's unit is written, and every other beat's unit stays as it is. Another beat that could not
  arm no longer refuses the emission. An unknown label refuses with the labels `nika.yaml` gives.
