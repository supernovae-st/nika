- **Secret teachings no longer offer the withdrawn `vault` source.** The refusals for a missing or
  unknown `source:` listed « vault · env · file », and the `secrets-store` hint said a `vault`
  secret would work « until vault resolution ships » and fail with NIKA-1702. Spec 01 §secrets
  withdrew `vault`, and the run reports the failure as NIKA-VAR-001. The teachings now list `env`
  and `file`, and the hint says `vault` is withdrawn from the contract and names NIKA-VAR-001. A
  `vault` secret still parses as before.
