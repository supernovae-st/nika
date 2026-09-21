- **`nika-compile` (ADR-137).** The stateless Compile core descends from
  `nika-onboard` at the 15k prod-LOC wall as the second member of the same unit
  (D-2026-07-09-N1). `nika-onboard` re-exports it at its historical
  `nika_onboard::compile` path; no caller, plan or provenance format changes.
