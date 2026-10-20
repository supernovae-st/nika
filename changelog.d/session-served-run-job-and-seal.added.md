- **The last run a Session keeps names its job, its receipt, its outputs and its verified seal.**
  For a run `nika serve` ran, the kept run now records the job it ran as, the receipt and the
  declared outputs that job serves (kept whole up to 16 KiB encoded; larger ones stay with the
  job, at `GET /v1/jobs/{id}`), and whether `nika trace verify` accepts the journal's
  `run_sealed` record and its signature (`seal_verified`, `signed`), never a seal read from the
  receipt's chain head. Forward
  compatibility: these parts are written only for served runs, and an engine older than this one
  reports such a kept run as unreadable and keeps its bytes unchanged, so nothing is lost; the
  kept value of a native run is unchanged.
