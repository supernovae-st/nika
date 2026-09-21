- **A seat the catalog limits to `json_object` is asked only for that.**
  The OpenAI-compatible wire forwarded `response_format: json_schema`
  verbatim and DeepSeek refused it at the door (HTTP 400, no token sampled),
  so the seated compile never answered. The wire now reads the catalog's
  per-model `json_mode`: `Object` sends `json_object` with the schema
  rendered into the last user turn and a generic instruction that every
  enum, every required property and nothing outside the schema is the law;
  `Unavailable` sends no `response_format`; `Schema` or an absent fact keeps
  the native mode. Enforcement stays local, in the verb's gate and the
  compiler's decoder.
