- **File journals refuse bytes beyond the verifier's existing bounds.**
  Check final encoded lines and cumulative bytes, including newlines,
  before writing. A refusal stops that journal and reports lost evidence
  while preserving the primary output and runtime settlement. This does
  not provide complete proofs for oversized runs or resolve #1458.
