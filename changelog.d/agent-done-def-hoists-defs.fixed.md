- **A typed `agent:` task's `nika:done` definition keeps an internal `$ref`
  resolvable on the wire.** The declared `schema:` rides nested under the
  sentinel's `result` parameter, and a JSON pointer such as
  `"$ref": "#/$defs/row"` resolves against the document root, which on the
  wire is that wrapper: the definition now hoists the schema's
  `$defs`/`definitions` to the wrapper's root and drops a `$schema` keyword,
  so a seat that validates its tool input follows the pointer to the
  definition instead of to nothing. A schema without definitions renders
  exactly as before, and local validation is unchanged.
