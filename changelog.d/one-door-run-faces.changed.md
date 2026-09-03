- **The run's faces say the whole truth.** A failed task terminal now
  carries the lane that failed (`model` · `provider` · `access` ·
  `access_id` · `billing`, and a note naming the model instead of `?`);
  `run_settled` carries an `error` on every failed frame, a launch
  refusal included (`NIKA-1800` · `NIKA-1801` · a missing input), and the
  `access_plan` rows of the lanes the run executed; `nika run --help`
  ends on its exit-code ladder; the resume that would switch access
  silently refuses as `NIKA-1807` (with `nika explain`); « wrote
  .nika/traces » prints only when a trace exists; `check --help` no
  longer claims a mistyped flag exits 3.
