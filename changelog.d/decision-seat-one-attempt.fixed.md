- **A `provider/name` decision seat is stated as sending each request once.** The compile
  receipt and the `--decision-model` help said its client kept protocol retries; the provider
  transport no longer re-sends anything, so the receipt now says « its own provider client, one
  attempt per request ».
