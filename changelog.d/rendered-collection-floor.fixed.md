- **`--max-cost-usd` now prices a `for_each` over an input at the items the
  invocation gives it.** With `inputs.xs` declaring a one-item default, a run
  given five items (`--inputs-json` or `--var`) used to be priced at one call,
  so it started under a cap that five calls cross. With a five-item default,
  a run given one item was refused although that call fits. The run's launch
  gate and the CLI's own preflight now bind the input as the run itself does:
  the given value replaces the default, and it never falls back to it. The
  floor counts the given items, so the first run is refused before any task
  (NIKA-1709) and the second is admitted. An explicit empty list prices zero
  calls. A value that is not a list is an unknown count, never the default's;
  the CLI already refuses such a mistyped value before any gate. `--model`
  still seats the envelope first. Literal lists, consts and inputs left to
  their default keep their counts.
