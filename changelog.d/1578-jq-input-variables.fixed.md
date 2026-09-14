- **`nika:jq` binds the keys of an object `input:` as jq variables.** An
  identifier-shaped key becomes `$key` (the `jq --argjson` shape), so « stamp a
  sibling onto every row » is `$rows | map(. + {batch: $batch})` instead of the
  `. as $in` dance or the silent `null` that `.batch` read inside `map` yields
  under jq's own semantics, which are unchanged; `.` stays the whole input and
  the run-start clock variable is never shadowed (#1578).
