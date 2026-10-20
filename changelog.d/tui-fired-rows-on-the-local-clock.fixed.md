- **A schedule's fired runs read on the clocks the person reads.** The schedule's line and its list
  entry said its slots and catch-ups in UTC (« dernier créneau 2026-10-16 06:00 UTC »). A slot now
  reads on its own schedule's clock, the zone its cadence declares, that zone named where it reads
  otherwise than the machine's (« dernier créneau 16/10 08:00, heure de Paris », « 16/10 08:00
  Paris » in the list), while a catch-up reads on the machine's clock (« rattrapé à 16/10 12:00 »),
  dated as the runs are. The zone is read from the embedded tzdb, never the host's.
