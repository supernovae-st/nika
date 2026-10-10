- **A provider call is never re-sent by the transport, and every receipt counts what was sent.**
  On a 429, 503 or 529 the provider layer re-sent the same request up to three more times
  (after 1, 2 and 4 s), so one authoring call on a busy route sent four identical 772 KB requests
  while its receipt said one call, and the money admission, which counts only the author's
  `retry:` and schema re-asks, never counted them. The call now ends at its first answer with a
  transient rejection; `nika_providers::transient_rejection` reads its status and the delay the
  provider named, for the author's `retry:` or the Session to decide. Every provider client also
  makes one physical attempt per post (no protocol-NACK replay). The fetch plane is unchanged.
