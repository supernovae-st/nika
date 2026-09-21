- **A fan-out over an empty corpus fails loudly instead of running green on nothing.**
  A `source_glob` that matched no file let the fan-out iterate zero times, the fold
  write an empty document and the run finish with exit 0 and no model call. The
  assembler now emits `glob_found` (`length > 0`) and `glob_admit` (`nika:assert`)
  between the glob and the reads, so an empty match stops the run with its reason.
