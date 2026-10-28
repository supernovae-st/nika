- **The SDK guard reads what the published `@supernovae-st/nika` 0.124.0 closes.**
  `published_sdk.json` is re-read from that package's `dist`, and every closed set of 0.123.2
  is unchanged. The one runtime change in that `dist` names three more native task frames
  (`task_skipped`, `task_cancelled`, `task_cache_hit`) in a mapping that reads any other frame
  as an engine event, so it closes nothing new.
