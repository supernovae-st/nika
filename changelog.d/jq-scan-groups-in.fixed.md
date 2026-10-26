- **`scan` and `IN` in jq expressions give jq's results.** `scan` with a
  regex that has capture groups returned each whole match where jq returns
  the captures (`"abab" | [scan("(a)(b)")]` gave `["ab","ab"]` instead of
  `[["a","b"],["a","b"]]`), and `IN(...)` was an undefined filter. `IN/1`,
  `IN/2` and `scan` over a match where every group takes part now follow
  jq 1.7 in `nika:jq`, output bindings, `nika check` and the compile
  verifier, which load the same definitions. A group that takes no part in
  a match (`a(b)?` on `a`, one side of `(a)|(b)`) still differs: jq yields
  `null` for it, while this engine leaves it out, and yields the whole match
  when no group took part.
