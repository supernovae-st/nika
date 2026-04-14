# ADR-012: Typestate for `nika-runtime` workflow lifecycle

**Status:** Accepted (spec — locks pattern before `nika-runtime` admission)
**Date:** 2026-04-14
**Phase:** Phase 3 (locks now, implements at admission)
**Deciders:** @ThibautMelen

## Context

A workflow in Nika transitions through four distinct states: parsed → validated → resolved → executable. Legacy Nika tracked this at runtime with a `WorkflowState` enum — users could call `.execute()` on any workflow and the runtime would return an error if the state was wrong. The failure mode is late, at call time, after side effects may already have kicked in.

Typestate (PhantomData-parameterized structs, state transitions consuming `self`) is now canonical in serious Rust codebases: axum 0.8 (handler typestate), Embassy (HAL driver typestate), rustls (TLS handshake typestate), hyper 1.x (connection state). It turns a class of runtime bugs into compile errors.

Committing the pattern **before** `nika-runtime` admits prevents the team writing a `WorkflowState` enum first and retrofitting typestate later.

## Decision

`nika-runtime`'s public `Workflow<S>` type is parameterized by lifecycle state. States:

```rust
pub struct Workflow<S: WorkflowState> { /* ... */ _s: PhantomData<S> }

// Marker types (zero-sized)
pub struct Parsed;     pub struct Validated;
pub struct Resolved;   pub struct Executable;

// Transitions consume self:
impl Workflow<Parsed> {
    pub fn validate(self, catalog: &Catalog) -> Result<Workflow<Validated>, NikaError>;
}
impl Workflow<Validated> {
    pub fn resolve(self, ctx: &ResolveContext) -> Result<Workflow<Resolved>, NikaError>;
}
impl Workflow<Resolved> {
    pub fn bind_kernel(self, kernel: &dyn Kernel) -> Workflow<Executable>;
}
impl Workflow<Executable> {
    pub async fn execute(self) -> RunReport;
}
```

`.execute()` exists **only** on `Workflow<Executable>`. It is a compile error to call it on any other state.

## Consequences

### Positive
- "Did we validate before running?" becomes a type-system question, not a runtime check.
- Skips the class of bugs flagged in shadow zone #1 (`nika serve` input trust — skipped validation path).
- `nika-cli` flow becomes `parse(...)?.validate(&cat)?.resolve(&ctx)?.bind_kernel(&k).execute().await` — self-documenting.

### Negative
- Learning curve for new contributors. Mitigated by an example in `docs/crate-specs/nika-runtime.md`.
- Some gymnastics if code wants to "keep a workflow around" in generic state — solved by `Box<dyn AnyWorkflow>` erasure trait where needed.

### Neutral
- Builder-style typestate adds surface compared to a single struct. Worth the compile-time safety.

## Evidence

- Pattern sources: axum 0.8 typed-extractor `Handler<T>`, Embassy HAL `Uart<NotInitialized> → Uart<Initialized>`, rustls `ServerConnection<Awaiting*>`
- Shadow zone #1 (`PRE_LAUNCH_GATES.md`) — HTTP-sourced inputs auto-trusted, partially mitigated by typestate enforcement of validation step

## Alternatives considered

### Alt A — Runtime enum (legacy approach)
Rejected — pushes the check to call time.

### Alt B — Builder pattern without typestate
Rejected — builder returns same type regardless of state, so compile-time guard is lost.

### Alt C — Phantom-lifetime instead of phantom-type
Rejected — less expressive for 4-state machine.

## Related

- ADR-007 — forward-compat invariants (each state variant is `#[non_exhaustive]`)
- ADR-006 — kernel ISP traits (`bind_kernel` takes `&dyn Kernel` per sealed-trait pattern, see ADR-014)
- `docs/crate-specs/nika-runtime.md` (to be created before admission)

## Notes

During `nika-runtime` admission spec (Phase 3), define `AnyWorkflow` erasure trait for the rare cases where state needs to be stored in a collection. Until then, keep the typestate rigid.
