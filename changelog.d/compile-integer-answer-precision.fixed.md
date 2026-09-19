- **Compile refuses integer answers it cannot hold exactly.** An integer
  literal above `u64::MAX` or below `i64::MIN` was decoded to a rounded f64
  before any guard ran and came back Ready, even into a `type: integer`
  constant: `18446744073709551616` was emitted as `1.8446744073709552e+19`.
  Every answer door (CREATE answers and the text, answer and structured EDIT
  inputs) now judges the answer's own text and refuses an integer token
  outside the exact `i64` range at any depth, naming the token and leaving
  the source or skeleton unchanged. Fraction and exponent answers remain
  floats and quoted digits remain text; this is a refusal, not arbitrary
  precision.
