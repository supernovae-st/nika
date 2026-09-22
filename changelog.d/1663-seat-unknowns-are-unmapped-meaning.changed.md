- **A seat's `unknowns` are unmapped meaning, never the values the compiler asks itself.**
  The authoring instruction now says so: connection details, credentials, endpoints,
  locations, field names, formats, thresholds, criteria and open policies are the
  compiler's own typed questions or bindings outside the program, and an effect the
  requester leaves undecided is an effect with policy `unspecified`. Measured on eco-60
  with gpt-5-mini before the change: 29 of 52 proposals ended in the catch-all
  `intent.clarification` because the seat listed such values as unresolved work.
