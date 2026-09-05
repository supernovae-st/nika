- **File journals refuse bytes beyond the verifier's existing bounds.**
  Check final encoded lines and cumulative bytes, including newlines,
  before writing. A refusal stops that journal and reports lost evidence
  while preserving the primary output and runtime settlement. This does
  not provide complete proofs for oversized runs or resolve #1458.
- **Incomplete trace diagnostics describe evidence, not runtime failure.**
  A missing terminal frame and an unheld writer lease no longer claim a
  crash or an unsettled run. An invalid final line also does not exclude
  intentional modification. A file journal can stop while the primary
  run still succeeds; verification tiers and exit codes are unchanged.
