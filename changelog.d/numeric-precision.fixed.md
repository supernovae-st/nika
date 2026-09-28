- **Numbers stay exact through generated workflows, or the run stops before anything is
  written.**
  - A workflow that reads a JSON file no longer changes numbers silently between its steps. A
    very large identifier used to be rewritten as `1.2345678901234568e29`, and a fine decimal
    such as `1.000000000000000001` as `1.0`, with the run still succeeding.
  - Any number the workflow may read or write now either passes unchanged or stops the run at
    the decode, naming the number and what it would have become.
  - Records a lookup selects are checked the same way.
  - Ordinary numbers (integers, decimals like `0.1`, `2.5e-3`, `1e2`) pass exactly as before.
