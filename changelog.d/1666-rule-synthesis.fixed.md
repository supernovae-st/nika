- **A numeric or equality rule the request states in words is the jq the
  workflow runs; `const.rule_expression` is no longer asked for it.** The
  compiler-arena tournament (40 sealed seeds) asked `const.rule_expression` for
  13 rules the intent already stated ("keep only the rows whose amount is
  strictly greater than 100", "cuya cantidad es menor que 10", "whose status is
  refunded"). A compute step over one parsed source (CSV, JSON, YAML, TOML) is
  now synthesized deterministically from a closed grammar: the FIELD is an
  identifier token, a word of the columns hint the request states ("columns
  order_id,customer,amount,status", "colonnes …", "(sku,nome,quantidade,minimo)"),
  the noun phrase between a relative pronoun ("whose", "dont", "cuya", "la cui",
  "cuja", "deren") and the comparison, or the word left of a symbol; the
  COMPARATOR is a symbol or a multilingual cue (EN · FR · ES · IT · PT · DE,
  including "at least / au moins / al menos / almeno / pelo menos / mindestens"
  and their "at most" twins), with a copula alone reading as equality and a
  negation as inequality; the VALUE is a number (compared after `tonumber`), a
  quoted or bare word for equality (exact, case-sensitive), or a second column
  (`.quantita < .soglia_minima`); clauses join through one conjunction ("and /
  et / y / e / und", "or / ou / o / oder"). The compute stage becomes
  `[.records[] | select(<predicate>)]`, preceded by a `compute_guard` jq and a
  `compute_admit` assert that fail loudly, naming the column, when the first
  parsed record lacks a referenced field, so a wrong column never filters
  everything in silence; the summary stage keeps counting and totalling the
  filtered rows, and `provenance.decision.rule` records the text, fields,
  comparator, value, jq and `synthesized: true`. A count-or-total request folded
  into the same rule ("and how many rows were kept and the total of their
  amounts", as a live authoring seat wrote it) is what the summary stage
  computes: the rule is still synthesized and `{count, totals}` becomes the
  `summary` output. Anything outside the grammar
  (a grouping in the same sentence, a number bounded by a size or attempt unit,
  a field no column names, mixed conjunctions) still asks `const.rule_expression`,
  and an explicit answer always wins over the synthesis: nothing is guessed.
