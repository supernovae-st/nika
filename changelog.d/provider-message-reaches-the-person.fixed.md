- **A provider's refusal reaches the person in the provider's own words, never with the key.**
  A provider failure kept only its status and recognised identifiers, so an exhausted Anthropic
  balance (HTTP 400 with "Your credit balance is too low" in its message) read only "provider
  API error (HTTP 400); type=invalid_request_error". The error now adds `the provider said:
  "…"`: the provider's message as one line of at most 400 characters, without control or
  invisible characters, with the key the call sent and every credential-shaped word withheld.
  The words are relayed, not classified: transience, quota and retry still read only the status
  and the recognised identifiers. The compile server keeps provider text from remote clients.
