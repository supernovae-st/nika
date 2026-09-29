- **Run traces name provider routes by origin.** A task's `inference_calls` and
  `pricing_route` fields now carry the durable projection of each call and its
  pricing provenance: a route is `{provider, model, origin}` and the requested
  endpoint is `requested_origin`, so no endpoint path, query or userinfo is
  written. Usage, estimates and states are kept as recorded, and `estimate_known`
  says whether the call was debited or counted unpriced. The named free text (a
  declared tariff's `billing_provider`, `provenance` and `version`, a call's
  `request_id` and `response_model`) is withheld when it holds endpoint material,
  and a `withheld` list names the field, never the text. Cost attribution keys
  (`cost_by_source`, `spend.by_source`, the `nika:inspect` cost view) read
  `provider/model @ origin`, and routes of one origin sum under one key; totals
  and call counts are unchanged, and pricing and identity keep the exact endpoint
  in memory. `inference_calls` no longer decodes as the in-memory call record;
  `inference_admission` and the cost journal are unchanged until a later slice.
