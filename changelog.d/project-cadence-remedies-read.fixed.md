- **The project file's refusals teach a cadence the arming door reads.**
  A beat with no or an empty `cadence:` was told to write
  `cadence: "dimanche 18h07"`, which `nika arm` and `nika serve` refuse for
  its missing zone. The remedy now shows `TZ=Europe/Paris dimanche 18h07`
  and the cron form with its zone.
