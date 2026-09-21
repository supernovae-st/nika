- **`nika-compile-reader` (ADR-138).** The frozen deterministic reader and the
  typed plan it produces descend from `nika-compile` at the 15k prod-LOC wall as
  the third member of the `nika-onboard` unit (D-2026-07-09-N1). `nika-compile`
  reads them at their historical module paths and keeps its whole public
  surface; no caller, plan or provenance format changes.
