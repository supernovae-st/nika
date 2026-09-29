- **Rankings, filters and sorts compare numbers exactly.**
  - « Keep the top 2 rows by points » over `1.000000000000000001`, `…003` and `…002` used to
    keep a different wrong pair depending on the order of the input rows. It now keeps `…003`
    and `…002` whatever the order.
  - A threshold is compared as the request states it.
  - When a ranking's cut falls between different records with the same value, the run stops
    instead of picking one by input order. Exact copies of a record still count as that
    record.
  - Plans saved before this change replay unchanged and gain the exact comparison.
