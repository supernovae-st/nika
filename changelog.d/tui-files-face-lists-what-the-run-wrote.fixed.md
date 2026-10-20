- **A run's files face and its card now list every file the run wrote.** The face said « wrote
  no file » while the run had written three files with a `for_each` of `nika:write`, and again on
  the leg resumed after a gate, whose written file comes back as a cache hit. A task now counts
  as a write when its note names `nika:write` or when the bytes the run names declare it, and
  each path its output names is listed, every path of a fan-out included. A path is only ever
  read from the write's own output.
