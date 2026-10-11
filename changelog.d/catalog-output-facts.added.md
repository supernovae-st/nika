- **`nika-catalog` states a model's output facts.**
  `model_output_limit` returns the output limit the catalog records with
  where it is recorded, `generated_cap` the cap a caller generates for a
  reasoning model within that limit, and `output_caps` the facts an
  authoring seat reads before it writes (reasoning recorded or not, the
  documented effort levels, the suggested cap). `nika-compile` now reads
  them there; no record changed.
