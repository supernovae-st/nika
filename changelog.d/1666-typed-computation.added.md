- **A compute step is stated as a typed computation, never as jq the model
  writes.** The proposal states a row filter (clauses, keep/drop polarity,
  junction), a group column with aggregates (sum, count, avg, min, max) under
  the output names the request states and the decimals it rounds to, derived
  outputs (arithmetic over other outputs), a sort and a projection. The compiler
  validates every column, literal and output name against the request and lowers
  the stages in one fixed order to jq; the guard still proves every source column
  on the first record. A CSV written from a grouped or projected computation
  carries the produced columns; totals are exposed one by one as the outputs the
  request named. The closed rule grammar stays as a fallback only.
