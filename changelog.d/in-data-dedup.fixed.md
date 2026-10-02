- **Stop asking for a cross-run state file when a request removes duplicates within its rows.**
  « Déduplique par customer et invoice_id, première occurrence conservée » or « Deduplicate by
  customer and invoice_id, keeping the first occurrence » was read as « no second effect for the
  same incoming identifier », and the round asked which JSON file keeps the identifiers already
  processed. A clause that scopes the removal by named fields and states the occurrence kept is
  now read as an operation when the request holds no cross-run cue: the seat's `distinct_by` or
  its program carries it, the final judge still checks it on the final bytes, the plan records the
  clause as an `in_data_dedup` binding, and an Applied finding says that no state across runs is
  asked. A removal beside a cross-run cue (« already processed », « déjà traité », « never
  twice », « between runs »), over events, callbacks or webhooks, or naming no kept occurrence or
  no field, keeps the obligation and its state-file question.
