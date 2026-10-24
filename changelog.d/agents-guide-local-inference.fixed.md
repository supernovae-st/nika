- **The `AGENTS.md` that `nika init` writes no longer says the shipped binary cannot serve models.**
  It said `nika model serve` « needs a `local-infer` build — the default binary prints the build
  recipe », but release binaries are built with `local-infer`, and 0.123.0's `nika doctor` reports
  « local inference built in ». The guide now says release binaries include it and that only a
  source build without the feature prints the recipe.
