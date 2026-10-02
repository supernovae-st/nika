- **Cost observations have a durable form, and unknown-cost routes are named
  by origin and bound only when canonical.**
  `nika_providers::project_observation` projects an account's cost
  observation (`nika/inference-cost-observation@1`) to `@2`: every endpoint
  becomes its origin; money, counters, states and ids are copied as written;
  and the named free text (a declared tariff's `billing_provider`,
  `provenance` and `version`, each attempt's `request_id`, `response_model`
  and `note`, and the `refusal`) becomes null when it holds endpoint
  material, with `withheld` naming the field by its instance pointer and
  counting unknown keys without naming them.
  `InferenceReceipt::durable_observation()` returns it while `observation()`
  keeps the exact `@1`. A well-formed `@2` projects to itself and a malformed
  one to nothing, and the admission reading law reads `@2` only when it is
  well formed. `project_route` and the `origin()` of a choice, an attempt and
  a billing route name an origin. The Run terminal receipt, new
  cost-journal observations and new Session history entries write `@2`;
  entries already recorded are kept as written. An unknown-cost route whose
  configured endpoint is not canonical (a form the URL parser would rewrite,
  or one with userinfo, a query or a fragment) is now refused before any
  review, naming at most its origin; configure the endpoint as the parser
  writes it, with a path. The review and challenge screens and the served
  review document name the route by the parser's origin, never its path.
