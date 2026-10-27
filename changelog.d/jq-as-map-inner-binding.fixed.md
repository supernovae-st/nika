- **`[jq-as-map]` no longer fires on a binding whose scope has ended.**
  `map(. as $r | {n: ($r.a | length)})` binds each element, so the map that
  encloses the binding maps what it was written to map, and so does a later
  map of its output (`map(. as $r | …) | map(.n)`); the run's result was
  right. The hint only checked that a `. as $name` binding and a bare `map(`
  both appeared; it now counts a bare `map(` only inside a dot binding's
  scope, which ends at the bracket that closes the construct the binding sits
  in, so `. as $c | map(.n)` still draws it.
