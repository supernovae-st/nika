- **Preserve the release coordinate in metadata writes.** The OCI marker and
  final publication PATCHes explicitly carry their already-verified tag and
  target SHA, avoiding the body-only update that orphaned the 0.118.5 draft.
  Post-write identity drift still refuses without rebinding or retrying;
  ambiguous writes no longer claim that nothing committed.
