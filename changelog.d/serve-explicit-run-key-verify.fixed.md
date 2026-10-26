- **`nika serve` attributes the journals it seals with an explicit run key
  pair.** A resident started with `NIKA_RUN_KEY_FILE` and `NIKA_RUN_PUB_FILE`
  naming a pair outside `~/.nika/keys` sealed its jobs with that pair, but
  `GET /v1/jobs/{id}/trace/verify` called the seal unattributable and both
  artifact routes refused the job with `409 artifact_journal_unverified`.
  The verifier now weighs that pair's public half after `--key` and before
  `~/.nika/keys/run-signing.pub` and the retired-key ledger, the order the
  signer takes them, so `nika trace verify` also attributes such a journal
  without `--key`. A host that names no pair verifies as before.
