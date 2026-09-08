- **Typed agent completion keeps definitions inside their schema resource.**
  When the output schema has a resource `$id`, `nika:done` keeps its `$defs`
  and `definitions` with that resource so local references resolve in the
  exposed tool definition. Schemas without a resource ID retain the existing
  wrapper projection; final-output validation is unchanged.
