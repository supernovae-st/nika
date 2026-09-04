- **Retain the draft release ID returned by creation.** A committed draft
  no longer depends on immediate visibility in the release list. The POST
  response owns the next read, which rechecks its immutable ID, exact tag,
  prerelease state and resolved tag commit before publication can proceed.
  Delayed-list and malformed-response regressions reproduce the failure of
  the unpublished v0.118.1 train; existing tags remain immutable.
