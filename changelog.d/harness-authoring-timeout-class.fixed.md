- **A harness authoring call that runs out of time is reported as a
  timeout.** When the call's own deadline passed first, its error was a
  generic provider error (NIKA-339) and the per-call receipt said
  `provider_error` while the harness record said `timed_out`. It is now
  the established timeout form, API 408 under NIKA-330, which the receipt
  and the call observer both read as `timeout`, behind a call ledger or
  not. A refusal whose words merely say it timed out stays a provider
  error, and the deadline itself is unchanged.
