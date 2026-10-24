- **`nika trace verify --help` lists every exit the verdict returns.** The help named exits 0, 2 and
  3, while a journal that never reached a terminal frame (a run in flight or a writer that died)
  exits 5 INCOMPLETE. It now lists 5, names buried seals and replay divergence under exit 2, and
  counts its four named refusals as four.
