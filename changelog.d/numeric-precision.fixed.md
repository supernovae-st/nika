- **Numbers stay exact through generated workflows, or the run stops before anything is
  written.**
  - A workflow that reads a JSON file no longer changes numbers silently between its steps. A
    very large identifier used to be rewritten as `1.2345678901234568e29`, and a fine decimal
    such as `1.000000000000000001` as `1.0`, with the run still succeeding.
  - Any number the workflow may read or write now either passes unchanged or stops the run at
    the decode, naming the number and what it would have become.
  - Records a lookup selects are checked the same way.
  - Ordinary numbers (integers, decimals like `0.1`, `2.5e-3`, `1e2`) pass exactly as before.
- **Rankings, filters and sorts compare numbers exactly.**
  - « Keep the top 2 rows by points » over `1.000000000000000001`, `…003` and `…002` used to
    keep a different wrong pair depending on the order of the input rows. It now keeps `…003`
    and `…002` whatever the order.
  - A threshold is compared as the request states it.
  - When a ranking's cut falls between different records with the same value, the run stops
    instead of picking one by input order. Exact copies of a record still count as that
    record.
  - Plans saved before this change replay unchanged and gain the exact comparison.
