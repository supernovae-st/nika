- **A draft that only serializes computed rows is not assembled.** A seat proposes « prepare
  the filtered CSV content for writing », « serialize the resulting array as JSON », « schreib
  das Ergebnis nach … » beside a computation: that is no language work, the write takes the
  computed rows as they are, and no model is asked. A detail that names language work (a
  summary, a note, a digest, headings, a reply, a translation) stays a draft. Measured on the
  sealed-v3 treatment lane: three such drafts ran under gpt-5-mini with a 4096-token cap and
  failed at runtime; three more waited on a `model` the request never needed. A format such a request states (« avec les mêmes colonnes et dans le
  même ordre », « mismas columnas ») is realized by the compute task, which keeps the source
  columns by construction, instead of waiting on a draft's prompt guidance.
