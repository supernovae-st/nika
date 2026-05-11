---
id: ADR-014
title: "Sealed kernel traits with explicit adapter registration"
status: accepted
date: "2026-04-14"
phase: "Phase 2 (first non-mock impl)"
deciders: ["@ThibautMelen"]
tags: ["kernel", "sealed-traits", "api-stability"]
affects_crates: ["nika-kernel", "nika-kernel-mock"]
affects_layers: ["L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-012", "ADR-016", "ADR-020", "ADR-028", "ADR-034", "ADR-039", "ADR-040", "ADR-041", "ADR-078", "ADR-079", "ADR-080"]
requires: ["ADR-006"]
enables: []
amends: []
fci: ["FCI-006"]
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.80.0, Phase 2 -- sealing before first L1 admission"
follow_ups: ["Apply sealed pattern in single commit before Phase 2 Session 1"]
---

# ADR-014: Sealed kernel traits with explicit adapter registration

## Context

`nika-kernel` (L0.5) defines ~20 public async traits — `Provider`, `MemoryStore`, `ToolExecutor`, `FsRead`, `HttpGet`, `ShellExecutor`, etc. Today only `nika-kernel-mock` implements them (L0.5 test). As Phase 2 admits L1 effect crates (`nika-http`, `nika-fs`, `nika-process`), each trait gains real implementors.

**The open question**: can an *external* crate (outside supernovae-st/nika workspace) implement these traits? If yes, every method signature becomes load-bearing across semver — adding a default-method parameter is a breaking change for external implementors. If no, we preserve flexibility to evolve method signatures as we learn.

2026 Rust practice (axum, serde, uv) uses the **sealed-trait pattern** (`pub trait Foo: private::Sealed { ... }`) to prevent external impls while keeping the trait public for users who depend on it via `dyn Foo`. rust-analyzer, rustls, tokio all seal core traits.

## Decision

All public traits in `nika-kernel` are **sealed** via the `private::Sealed` supertrait pattern:

```rust
// nika-kernel/src/lib.rs
mod private {
    pub trait Sealed {}
}

// nika-kernel/src/provider.rs
pub trait ProviderInfer: private::Sealed + Send + Sync { /* ... */ }

// nika-kernel/src/provider.rs (internal, for blanket Sealed impls)
impl<T: crate::Provider + ?Sized> private::Sealed for T {}
```

External crates **cannot** `impl ProviderInfer for MyType` (compile error: `Sealed` not implemented, and `Sealed` is private). Instead, external integrations route through:

1. **Adapter macros** (`nika_kernel::provider_impl!`) for workspace-local crates (`nika-provider-rig`, `nika-provider-native`, `nika-provider-mock`).
2. **`nika-builtin-*` wrappers** for ecosystem integrations (GitHub, Cloud, Workspace — ADR tbd).
3. **Future pck packages** (plugin protocol, Phase 4+) for community extensions.

Adding a trait method still requires care, but the set of implementors is **finite and controlled by the workspace**.

## Consequences

### Positive
- Method signatures and default methods can evolve within v0.x without breaking external code (there isn't any external impl by construction).
- Clear path for ecosystem extensions (adapter macros > plugin protocol > pck) — enforced architecturally.
- Matches Rust 2026 canonical pattern.

### Negative
- Some friction for hypothetical out-of-workspace adapters — but there are none today, and when they emerge they belong as `nika-builtin-*` or pck packages per ADR-004's crate-count discipline.
- Adds a `private::Sealed` module + blanket impl per trait group.

### Neutral
- Language-level `sealed` keyword RFC is still unmerged as of 2026-Q1 — the supertrait pattern is idiomatic.

## Evidence

- Pattern sources: axum, serde (`private::Sealed`), rustls (`server::ServerConfig` sealed), tokio-stream
- `crates/nika-kernel/src/lib.rs:19-34` — trait hierarchy (to be updated with sealed pattern)

## Alternatives considered

### Alt A — Open traits (no sealing)
Rejected — every method change becomes potential breakage.

### Alt B — Opaque impl trait only (no trait at all)
Rejected — loses `dyn Trait` + breaks test substitutability (kernel-mock).

### Alt C — `#[non_exhaustive]` on the trait
Not valid in Rust — `#[non_exhaustive]` is structs/enums only.

## Related

- ADR-006 — kernel ISP traits (which this seals)
- ADR-012 — typestate runtime (uses sealed kernel via `bind_kernel(&dyn Kernel)`)

## Notes

Migration: sealing can be added to `nika-kernel` in a single commit before any L1 effect crate admits. No breaking change to `nika-kernel-mock` (workspace-local, blanket-Sealed applies). Plan for Phase 2 Session 1.
