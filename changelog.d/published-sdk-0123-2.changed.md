- **The SDK guard reads what the published `@supernovae-st/nika` 0.123.2 closes.**
  `published_sdk.json` is re-read from that package's `dist`. Every closed set of 0.123.1 is
  unchanged, and one is new: an artifact manifest's `verdict` must be `sealed` or `anchored`,
  the two values `GET /v1/jobs/{id}/artifacts` serves. The manifest and its entries are read
  open otherwise, so a later key rides through.
