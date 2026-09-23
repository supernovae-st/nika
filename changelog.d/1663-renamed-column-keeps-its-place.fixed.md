- **A renamed column keeps its place in the written CSV.** « benenne die Spalte kwh in
  energie_kwh um (sonst nichts ändern) » wrote `anlage,datum,energie_kwh`, the sorted keys of
  the rows, because the source's header order reached the CSV stage only when the rule renamed
  nothing (sealed sv3-41). The header order is now mapped through the renames the rule states
  under a `<stem>_columns` jq stage the CSV stage reads.
