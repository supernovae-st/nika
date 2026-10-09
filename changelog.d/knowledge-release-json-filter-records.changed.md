- **The embedded knowledge release offers a JSON record filter.** The
  current r2 release adds the admitted component
  `block:json-filter-records`, which reads a JSON array of records and
  keeps the ones a jq expression selects, whole and in source order; its
  path is the human's to bind, its expression the compiler's. Every
  checked row of the release is re-verified on the current engine. An
  explicit embedded pin to the release it replaces is refused by its
  identity; reopening a Session pins the new release and keeps its
  earlier receipts.
