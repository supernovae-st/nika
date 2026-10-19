- **One state of a run per screen: the settled outcome wins.** The project list showed `ok`
  beside a workflow whose last run had just failed, and the live line kept saying
  `Running … 15/15 tasks` while the run's own header said `settled · failed`. A workflow whose
  last run in this session failed now reads `run failed`, in the failure colour, until a newer
  run of it settles otherwise. Once a run settles, the live line says how it ended, as its card
  and its header do (`veille-tech.nika · Failed after 16.5s · 3 tasks · $0.034 · 1 model
  call`), then what the turn does now, and never that the run is still going.
