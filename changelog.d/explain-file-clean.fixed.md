- **`nika explain FILE` no longer says checks clean on a red check.**
  The human line and the JSON `clean` flag follow `CheckReport::is_clean`
  (PERMITS-red files say `check red` / `"clean": false`).
