---
id: ADR-006
title: "Layered architecture with kernel ISP atomic traits and trait_variant"
status: accepted
date: "2026-04-13"
phase: "Phase 1 (kernel admission)"
deciders: ["@ThibautMelen"]
tags: ["architecture", "kernel", "traits", "isp", "layers", "trait-variant"]
affects_crates: ["nika-kernel", "nika-kernel-mock"]
affects_layers: ["L0", "L0.5", "L1", "L2", "L3", "L4", "L5"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-004", "ADR-007", "ADR-012", "ADR-013", "ADR-014", "ADR-016", "ADR-017", "ADR-018", "ADR-019", "ADR-020", "ADR-028", "ADR-034"]
requires: ["ADR-004"]
enables: ["ADR-007", "ADR-012", "ADR-013", "ADR-014"]
amends: []
fci: ["FCI-001", "FCI-006"]
inv: ["INV-016", "INV-017", "INV-027"]
shadow_zones: []
nika_codes: []
timeline: "v0.80.0, Phase 1"
follow_ups: []
---

# ADR-006: Layered architecture + kernel ISP atomic traits + `trait_variant`

## Context

The kernel layer defines contracts for all side effects (filesystem, HTTP, shell, memory, embeddings, tool execution, WASM host, observability). Legacy approach was coarse-grained: one large `Provider` trait, one `Fs` trait. Consumers that only needed "read a file" had to depend on the entire surface. `async_trait` was universally used, which boxes every future on the async dispatch path — measurable overhead on the inference hot path.

Rust 1.91 native AFIT (async fn in trait) + `trait_variant` crate (v0.1) unlocked a better pattern: write the async trait once, let `trait_variant::make` generate a companion `Dyn`-suffixed trait for object-safety without boxing on the static path.

## Decision

**Layered architecture** (strict dep direction):

| Layer | Purpose | Admitted |
|---|---|---|
| L0 | Primitives — error codes, catalog data, pure types | `nika-error`, `nika-catalog` |
| L0.5 | Kernel — abstract side-effect traits + request/response types | `nika-kernel`, `nika-kernel-mock` |
| L1 | Infrastructure — implementations of kernel traits (fs, http, providers, etc.) | pending Phase 2+ |
| L2 | Core runtime + DAG execution | pending Phase 2+ |
| L3 | Subsystems — binding, catalog overlay, policy | pending Phase 3+ |
| L4 | Applications — CLI, TUI, daemon, serve, LSP, verify tooling | `nika-catalog-verify` (binary) |
| L5 | Full product integration | pending |

Layer-level ordering is enforced by `scripts/ci/check_layers.rs` pattern (see ADR-009): `layer(dep) ≥ layer(crate)` is verified by parsing `Cargo.toml` deps against a table.

**Kernel trait design (L0.5)**:

- ~20 atomic ISP traits: `FsRead`, `FsWrite`, `FsMeta`, `FsList`, `HttpGet`, `HttpPost`, `ShellRun`, `ShellCancel`, `MemoryRemember`, `MemoryRecall`, `MemoryForget`, `ProviderInfer`, `ProviderStream`, `ProviderMeta`, `ProviderEmbed` (optional), `ProviderVision` (optional), `ToolExecutor`, `WasmPluginHost`, `ObservabilitySink`, `Clock` (sync by design).
- Grouped into ~6 super-traits via blanket impls: `Fs`, `HttpClient`, `ShellExecutor`, `MemoryStore`, `Provider`.
- All async traits use `#[trait_variant::make(Send)]` — native Rust 1.91 AFIT on static path, `Dyn`-suffixed companion trait for object safety.
- `BTreeMap` preferred over `HashMap` in kernel types for deterministic iteration (tests are reproducible).
- Cancel tokens passed as function parameters, not embedded in request structs (keeps structs runtime-agnostic).

## Consequences

### Positive
- A context loader that only reads files imports `FsRead` alone — compiler enforces minimum surface.
- Zero boxing on the hot inference path (static dispatch via native AFIT).
- Test hermeticity is structural: every I/O path is behind a trait, so `nika-kernel-mock` substitutes with zero network/disk. `nika-kernel` 99 tests + `nika-kernel-mock` 88 tests exercise this fully.
- Adding a new side effect = new atomic trait, no impact on existing ones.
- `Provider` mandatory-vs-optional split (`ProviderInfer + ProviderStream + ProviderMeta` mandatory; `ProviderEmbed + ProviderVision` optional) lets lightweight providers (e.g., mock) implement only what they support.

### Negative
- `trait_variant` generates a pair of traits per async trait, doubling the documented API surface — ~40 traits visible to `cargo doc` from ~20 source definitions.
- Caller ergonomics: implementing Provider requires 3 trait impls minimum (`Infer + Stream + Meta`). Mitigated by default methods where possible.
- `BTreeMap` perf in ultra-hot loops is slightly worse than `FxHashMap`; kernel isn't a hot loop, so acceptable.

### Neutral
- `Clock` sync-only is explicit YAGNI. Revisit if proven wrong.

## Evidence

- `crates/nika-kernel/src/lib.rs` lines 19–34 — trait hierarchy docstring + re-export list
- `crates/nika-kernel/src/ai/memory.rs` lines 272–305 — `MemoryRemember`, `MemoryRecall`, `MemoryForget` + blanket `MemoryStore` + `EmbeddingProvider` with `#[trait_variant::make(Send)]`
- `crates/nika-kernel/src/ai/provider.rs` — `InferRequest`/`InferResponse` with reserved `Option<MemoryDirective>` field (see ADR-007)
- `CHANGELOG.md` lines 169–215 — `nika-kernel` admission, key decisions listed, commit `ef8804371`
- Related crates: **`nika-kernel`** (L0.5 traits), **`nika-kernel-mock`** (L0.5 test mock)

## Alternatives considered

### Alt A — Coarse-grained traits (`Fs`, `Provider` mega-traits only)
Rejected: re-creates the legacy problem where consumers over-depend.

### Alt B — `async_trait` everywhere (boxed futures)
Rejected: measured ~100ns per call overhead; multiplied across a DAG with 100+ nodes becomes visible. Native AFIT is free.

### Alt C — No super-traits, only atomic traits
Rejected: 20 atomic trait bounds is unergonomic for consumers that genuinely need most of Fs/Provider. Blanket `Fs: FsRead + FsWrite + FsMeta + FsList` gives the best of both.

## Related

- ADR-003 — 12-gate admission (kernel admission went through all 12)
- ADR-004 — context-window sizing (kernel + kernel-mock together are 5,080 LOC)
- ADR-007 — forward-compat invariants (kernel pre-plants `MemoryStore`, `ToolExecutor` traits for v0.95 Cortex and v0.100 agent-v2 without touching kernel later)
- ADR-012 — typestate runtime (consumes the kernel via `bind_kernel(&dyn Kernel)`)
- ADR-013 — Loom concurrency verification (validates trait *impls* of the contracts this ADR defines)
- ADR-014 — sealed kernel traits (locks the trait surface this ADR introduced behind a `Sealed` super-trait)

## Notes

Rust 1.91 `trait_variant` is still v0.1.x. Keep version-pinned. Re-evaluate when the crate reaches v1.0 or if stdlib absorbs the pattern.

## Amendment 2026-04-16 — Kernel stays monolithic forever

The original ADR-006 reserved a future split into 4 sibling crates
(`nika-kernel-{core,ai,runtime,plugin}`) when LOC > 10k OR pub trait count
> 50. After multi-agent architectural review (rust-architect + rust-pro +
web-researcher SOTA + Explore file audit, 2026-04-16), this split-trigger is
**rescinded**.

`nika-kernel` stays as ONE crate forever, with 5 internal group modules
(`io/`, `ai/`, `runtime/`, `plugin/`, `infra/`) that already provide the
logical separation. Real Rust workspaces (rust-analyzer, helix, oxc, biome,
ruff, uv, bevy, tokio) keep their kernel-equivalents monolithic; only
polkadot-sdk splits and that's blockchain-feature-gated. Splitting would force
`use nika_kernel_ai::Provider; use nika_kernel_core::Clock;` everywhere — worse
DX. The Q7 prelude hub (added 2026-04-16) depends on kernel being whole.

**Trigger to revisit (both must hold):**
- kernel exceeds 15k LOC, AND
- 2+ active sub-domains being modified independently in same session for 3+
  consecutive sessions

Reserved crate names (nika-kernel-core/ai/runtime/plugin) freed.

See `~/.claude/.../memory/feedback_kernel_monolithic_forever.md` for the full
rationale + agent evidence.

Status of original split-trigger: **superseded by this amendment.**
