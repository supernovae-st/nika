- **`--answer task=yes` stays a string on `mode: input`.** Confirm
  (the stdlib default) still takes `yes`/`y`/`no`/`n` as the TTY
  boolean; an input gate binds `"yes"` so resume completes instead of
  re-pausing on a boolean the prompt cannot fill.
