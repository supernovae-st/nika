# ADR-005: Trait-based error hierarchy (`NikaErrorCode` + `Box<dyn>` + code registry)

**Status:** Accepted
**Date:** 2026-04-13
**Phase:** Phase 1 (first admitted crate)
**Deciders:** @ThibautMelen

## Context

Legacy Nika had a god `NikaError` enum with hundreds of variants spread across crates. Shadow-zone audit (2026-04-13) identified this as zone #6: **95% of variants lacked Display golden tests**, and error codes had silently collided. Downstream crates had to depend on the central enum, creating cyclic dep risks.

Three alternatives were weighed during `nika-error` admission:

- **A — `anyhow::Error` everywhere:** opaque, no structured codes, loses the wire-format IDs (`NIKA-140`, `NIKA-384`) that Shield and CLI rely on.
- **B — single `thiserror` enum in a root crate:** every crate must depend on the enum; adding a variant touches the root crate and rebuilds the world; circular-dep magnet.
- **C+ — trait-based hierarchy:** per-crate error enums implement a shared trait; unified at API boundaries via `Box<dyn>`.

## Decision

"Option C+" — trait-based error hierarchy.

- `NikaErrorCode` trait lives in `nika-error` (L0). Every crate's error enum implements it.
- `NikaError = Box<dyn NikaErrorCode>` — dynamic dispatch at API boundaries.
- `NikaCode` carries a **dual representation**: wire format `"NIKA-140"` for Display/logs AND a structured `{num: u16, category: Category, severity: Severity, slug: &'static str}` for JSON serialization and programmatic routing.
- `CoreError` handles cross-cutting errors that don't belong to any specific crate (config, bootstrap, etc.).
- **Error code ranges reserved per subsystem** in `docs/architecture/forward-compat-invariants.md`:

  | Range | Subsystem |
  |---|---|
  | NIKA-000..099 | Core |
  | NIKA-100..199 | MCP |
  | NIKA-200..299 | Shield (security) |
  | NIKA-300..399 | Catalog |
  | NIKA-400..499 | Cortex (memory) |
  | NIKA-500..599 | Providers |
  | NIKA-600..699 | Runtime / DAG |
  | NIKA-700..799 | IO (fs, http, shell) |
  | NIKA-800..899 | Observability |
  | NIKA-900..999 | Reserved |

- Default trait methods: `is_transient() -> bool` (for retry logic), `fingerprint() -> u64` (for dedup/ratelimit).
- Downcasting via `AsAny` supertrait avoids per-impl boilerplate.

## Consequences

### Positive
- Each crate owns its error enum without circular deps.
- `is_transient()` default enables retry logic across all errors without coupling the retry layer to specific error types.
- Error code ranges prevent collision as subsystems are admitted over 11+ months.
- Display parity enforced via golden tests per crate, resolving shadow zone #6.
- JSON serialization round-trips: `{num, category, severity, slug}` → wire format → back.

### Negative
- `Box<dyn>` allocation on the error path (vs zero-alloc enum with `thiserror`). Measured: ~50ns additional latency per error — acceptable; errors are exceptional by definition.
- Every new crate author must implement `NikaErrorCode` — more ceremony than `anyhow::bail!`. Mitigated by `derive`-style helpers in `nika-error`.

### Neutral
- `Box<dyn>` + `AsAny` supertrait means error types must be `'static`, `Send`, `Sync` — matches existing conventions.

## Evidence

- `tools/nika-error/src/traits.rs` lines 43–71 — `NikaErrorCode` trait definition with `is_transient`, `fingerprint` defaults
- `tools/nika-error/src/lib.rs` — full crate structure, 1,013 LOC, 44 tests, Gate 5 mutation 100%
- `docs/architecture/forward-compat-invariants.md` lines 105–127 — reserved error code ranges table
- `CHANGELOG.md` — `nika-error` admission commit `42909b1c7`
- Related crate: **`nika-error`** (first admitted, L0)

## Alternatives considered

### Alt A — `anyhow::Error` at API boundaries
See context. Rejected: opaque; loses wire-format IDs; breaks Shield integration.

### Alt B — Single `thiserror` enum in root
See context. Rejected: coupling + build-time cascade on any variant addition.

### Alt D — `snafu` with context selectors
Evaluated in Rust Council. Rejected: snafu's context pattern is ergonomic for small error trees but less flexible for dual wire/structured representation. Trait-based pattern scales better past 100 error variants.

## Related

- ADR-007 — forward-compat invariants (`#[non_exhaustive]` on error enums)
- ADR-010 — miette as L4 presentation (the rendering contract that wraps this structural contract)
- ADR-015 — `expect-test` inline snapshots (regression coverage for rendered error output)
- `tools/nika-error` — implementation
- `docs/architecture/forward-compat-invariants.md` — error code range registry

## Notes

If a subsystem exhausts its 100-number range (unlikely), extend into NIKA-900..999 reserved range with explicit registry update. Never overlap.
