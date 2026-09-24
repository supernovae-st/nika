- **`nika-compile-cognition` (ADR-140).** Seat authoring ascends to its own member;
  deterministic compilation and native record replay stay in core. Existing explicit
  imports through `nika_onboard::compile` remain available. Direct
  `nika_compile::{Cognition, NoProvider, compile_with_cognition, compile_with_provider, decide}`
  imports must move to `nika_compile_cognition` or the onboarding facade. Core adds the
  public `surface` module, mutable request and authoring-policy fields, and receipt/retrieval
  constructors. The extraction introduces no new Compile wire generation or SDK options.
