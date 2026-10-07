- **An explicit reasoning effort is admitted on the exact `deepseek/deepseek-flash`.**
  Its catalog entry now lists `low`, `high` and `max`, as the provider's model
  listing and documentation state for that exact name, so
  `NIKA_AUTHORING_REASONING=low` or `--authoring-reasoning low` is sent as
  `thinking.type=enabled` and `reasoning_effort=low` on its direct route, with
  the same output caps; it was refused before any byte. Nothing else inherits
  the levels: suffixed or dated names, old aliases, gateways and overridden
  endpoints still refuse an explicit level before sending, no default changed,
  and the effort a call was served stays unknown.
