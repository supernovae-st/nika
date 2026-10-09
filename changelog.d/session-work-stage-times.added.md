- **A Session snapshot says how long its knowledge and trial stages took.**
  `work.authoring.stages` now carries, as the compile's decision record
  states them, the wall time of the Foundry knowledge qualification
  (`qualification_ms`) and each trial of a candidate in order (`trials`:
  how far it went, `elapsed_ms` and the host's `runtime_bound_ms`). The JSON
  door and `/v1/sessions` carry it unchanged; a time the record does not
  state is null, nothing is summed, and no output or failure text passes.
