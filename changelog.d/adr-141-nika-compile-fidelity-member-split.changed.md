- **`nika-compile-fidelity` (ADR-141).** The laws a candidate document is judged by, the
  seat's sketch and the plan a candidate states ascend from `nika-compile-reader` at the 15k
  prod-LOC wall as a member of the `nika-onboard` unit (D-2026-07-09-N1). `nika-compile` reads
  them through the shared compiler unit, and the reader depends on `serde_json` alone
  again. Direct Rust users must move `nika_compile_reader::{candidate, fidelity, sketch}`
  imports to `nika_compile_fidelity`. Existing `nika_onboard::compile` entry points and
  the Compile machine wire remain available; the move introduces no new wire generation.
