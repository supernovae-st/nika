- **A reasoning effort the model does not offer is a finding, not « clean ».** A workflow
  declaring `run.reasoning.effort: low` for a model whose catalog lists no effort level passed
  « check · clean ✔ » and « Vérifié ✓ ». Every run of it was then refused before its first task
  (NIKA-1800). The Session's check now reports it as a finding, so such bytes are neither shown
  as ready nor saved as clean. What only this machine lacks, such as a seat not installed, stays
  a run-time matter.
