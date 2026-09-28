- **An unknown cost exposure now has a supported recovery door.**
  `nika trace cost` shows, as data (`--json`), each earlier Run whose billing
  is unknown and the digest of its latest journal row.
  `nika trace cost reconcile` appends one explicit resolution (billed, not
  billed or still unknown) tied to that exact digest and to the inspected
  project. The resolution is attributed to the local OS account and recorded as
  the operator's unverified attestation. Nothing is deleted or rewritten;
  preflight refusals leave the journal untouched. An append I/O failure
  keeps the write outcome uncertain. `still-unknown` keeps blocking, and a
  final resolution only lifts the block: the next unknown-cost Run still asks
  its own fresh question.
