- **An admission tariff can name the dated models its provider serves, and a
  cache-write rate.** `nika/inference-admission@1.2` adds two optional fields.
  `served_models` lists the models the provider's own documentation names as
  served for the selected model (a dated snapshot its replies report): a reply
  naming one of them answers that same route at settlement, in a restored
  checkpoint and in the per-dispatch record, and any other name leaves the
  charge unknown. `cache_write_nano_per_token` prices input written to the
  provider's cache, and a write rate above input reserves the whole context as
  written; a tariff without it leaves a call that wrote the cache unknown, now
  at settlement as well as in the per-dispatch record. No current row uses
  either field. Pricing observations name `table_schema` `@1.2` and a
  `cache_write_rate`, and records written under `@1.1` still read.
