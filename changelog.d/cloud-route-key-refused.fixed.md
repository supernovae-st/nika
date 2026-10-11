- **A refused cloud key reads « key refused », and a keyed route's own listing governs its
  offers.** A Session's census used to call every keyed cloud route ready from the key's presence
  alone: a key the provider had revoked was offered, and the offers came from the catalogue. The
  census now sends each keyed OpenAI-compatible route one bounded, authenticated
  `GET {base}/models` with its own key (about 3 s, all routes at once, never an inference): a 200
  lets the listing govern the route's offers, a 401 or 403 reads « key refused » with the
  provider's own words (the key withheld) and the route is not offered, and no answer leaves the
  key configured, not validated.
