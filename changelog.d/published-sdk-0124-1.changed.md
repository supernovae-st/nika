- **The SDK guard reads what the published `@supernovae-st/nika` 0.124.1 closes.**
  `published_sdk.json` is re-read from that package's `dist`, and every closed set of 0.124.0
  is unchanged. Its `dist/index.js`, `dist/index.cjs` and `dist/bin/nika.js` are byte for byte
  those of 0.124.0: only its type declarations, its bundled `openapi.json` (`info.version`),
  its README and its version metadata moved, so it closes nothing new.
