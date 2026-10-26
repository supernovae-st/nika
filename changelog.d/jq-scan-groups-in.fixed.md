- **`scan` and `IN` in jq expressions give jq's results.** `scan` with a
  regex that has capture groups returned each whole match where jq returns
  the captures (`"abab" | [scan("(a)(b)")]` gave `["ab","ab"]` instead of
  `[["a","b"],["a","b"]]`), and `IN(...)` was an undefined filter. Both now
  follow jq 1.7 in `nika:jq`, output bindings, `nika check` and the compile
  verifier, which load the same definitions.
