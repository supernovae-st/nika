- **`[jq-as-map]` no longer fires on a binding inside the map body.**
  `map(. as $r | {n: ($r.a | length)})` binds each element, so the map that
  encloses the binding maps what it was written to map, and the run's result
  was right. The hint only checked that a `. as $name` binding and a bare
  `map(` both appeared; it now counts a bare `map(` only after the first dot
  binding, so `. as $c | map(.n)` still draws it.
