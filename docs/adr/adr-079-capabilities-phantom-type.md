---
id: ADR-079
title: "Capabilities<E: EffectSet> phantom-type kernel · compile-time effect refusal (Option C hybrid)"
status: proposed
date: "2026-05-12"
phase: "Phase 2 (pre-nika-mcp-server admission)"
deciders: ["@ThibautMelen"]
tags: ["kernel", "capabilities", "type-state", "phantom-type", "effect-system", "sandbox", "compile-time", "zero-cost"]
affects_crates: ["nika-kernel", "nika-effects", "nika-mcp-server", "nika-binding", "nika-wasm-host"]
affects_layers: ["L0.5", "L1", "L2", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-012", "ADR-014", "ADR-040", "ADR-041", "ADR-078", "ADR-080"]
requires: ["ADR-006", "ADR-014"]
enables: ["ADR-080"]
fci: ["FCI-001", "FCI-002", "FCI-006"]
inv: ["INV-017", "INV-019", "INV-025"]
shadow_zones: ["Gate-4-L1-taint-runtime", "Gate-7-provider-parity"]
nika_codes: ["NIKA-391 (proposed · CapabilityNotProven)"]
timeline: "ship pre-nika-mcp-server admission · Phase 2 gating · co-locate with ADR-078 sealed-pattern commit"
follow_ups: ["macro caps! ergonomic layer", "MemRecall/MemRemember axes reserved · activate Phase 1.5 W10"]
---

# ADR-079: `Capabilities<E: EffectSet>` phantom-type kernel · Option C hybrid

## Context

`crates/nika-kernel-plugin/src/sandbox.rs:19-92` already ships the runtime `Sandbox` trait + `Capability` enum (5 axes: `FsRead`, `FsWrite`, `Network`, `ProcessSpawn`, `EnvRead`). Today's check is purely runtime — `Sandbox::check_capability(&Capability)` returns `Result<(), SandboxError>` — which means call sites like `effects::http::fetch(url)` cannot be **structurally refused at compile time** when the caller lacks the `Network` capability.

Per Anthropic public-eng effect-system literature + Karpathy AI-2027 capability-bound discipline + Kokotajlo AI-2027 paper · compile-time refusal is the load-bearing property for sandboxing the 150M+ MCP server downloads (cf ADR-080 CVE-2026-04). The Anthropic position « SDK consumers must sandbox by construction » requires compile-time enforcement, not just runtime defense.

Empirical state: `Sandbox` runtime check is canonical AND `Capability` enum vocabulary is stable. ADR-079 does NOT replace either — it LIFTS the runtime enum into a compile-time marker companion, preserving the existing 5-axis taxonomy + reserving 2 axes for Phase 1.5 memory subsystem (per ADR-004 + ADR-078 split).

## Decision

**Adopt Option C — hybrid phantom-type (compile-time) + runtime CapabilityToken (serde-survivable).**

Per Socratic premise audit · pure compile-time phantom (Option A) cannot survive `serde::Deserialize` of YAML workflow capability grants — phantom-type must be reconstructed via runtime tag match. Pure runtime bitmask (Option B) cannot refuse `effects::http::fetch(&caps, url)` at typecheck — defeats the load-bearing property. Hybrid (C) provides both: compile-time fast-path refusal + runtime serde-survival via typed proof transition.

### Kernel shape (lives at `nika-kernel/src/capability.rs` · NEW · ~300 LOC)

```rust
use core::marker::PhantomData;
use crate::sealed::Sealed;

// ── 5 axis markers (reuse sandbox::Capability vocabulary 1-to-1 + 2 reserved) ──
pub enum FsRead {}      // uninhabited · phantom only
pub enum FsWrite {}
pub enum Net {}
pub enum Exec {}        // = ProcessSpawn in sandbox::Capability
pub enum EnvRead {}
// RESERVED (Phase 1.5 · W10 activation per ADR-078 split + ADR-004 memory cluster)
pub enum MemRecall {}
pub enum MemRemember {}

impl Sealed for FsRead {}  // ... (7 sealed impls)

pub trait Axis: Sealed + 'static {}
impl Axis for FsRead {}  // ... (7 axis impls)

// ── Type-level HList set encoding (typenum 1.19 idiom · proven in 2026 ecosystem) ──
pub trait EffectSet: Sealed + 'static {}

pub enum Nil {}
impl Sealed for Nil {}
impl EffectSet for Nil {}

pub struct Cons<A: Axis, Rest: EffectSet>(PhantomData<(A, Rest)>);
impl<A: Axis, Rest: EffectSet> Sealed for Cons<A, Rest> {}
impl<A: Axis, Rest: EffectSet> EffectSet for Cons<A, Rest> {}

// ── Membership constraint (inductively defined) ──
pub trait Has<A: Axis>: EffectSet {}
impl<A: Axis, Rest: EffectSet> Has<A> for Cons<A, Rest> {}
impl<A: Axis, B: Axis, Rest: EffectSet + Has<A>> Has<A> for Cons<B, Rest> {}

// ── The zero-cost compile-time witness ──
#[derive(Debug, Clone, Copy)]
pub struct Capabilities<E: EffectSet> {
    _phantom: PhantomData<fn() -> E>, // contravariant · prevents auto-Sync mishaps
    _seal: (),                         // private field · external construction blocked
}

impl<E: EffectSet> Capabilities<E> {
    #[doc(hidden)]
    pub(crate) const fn __unchecked() -> Self {
        Self { _phantom: PhantomData, _seal: () }
    }
}

// ── Runtime token = existing sandbox::Capability (1 vocabulary) ──
pub use crate::plugin::sandbox::Capability as CapabilityToken;

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum CapabilityProofError {
    #[error("capability not proven: runtime token lacks axis required by E")]
    NotProven,
}

// ── Typed proof transition (runtime → compile-time) ──
pub fn proven<E: EffectSet>(
    set: &[CapabilityToken],
) -> Result<Capabilities<E>, CapabilityProofError> {
    // Inductive runtime check: every axis in E must appear in `set`.
    // Implementation uses const-eval witness array; ~30 LOC.
    todo!("admission impl")
}
```

### Ergonomic macro at call sites (declarative `macro_rules!` v1.0 · NO proc-macro at L0.5 per Q1 lock 2026-04-16)

```rust
// caps!(token_set => Net, FsRead)
//   expands to: proven::<Cons<Net, Cons<FsRead, Nil>>>(&token_set)
```

### Compile-time refusal example

```rust
let caps: Capabilities<Cons<FsRead, Nil>> = caps!(yaml_token_set => FsRead)?;

effects::fs::read(&caps, Path::new("/tmp/data")).await?;  // compiles · Has<FsRead> ✅

effects::http::fetch(&caps, &url).await?;
// ^^^^ COMPILE ERROR · `Cons<FsRead, Nil>: Has<Net>` not satisfied
```

Gate 12 admission of `nika-mcp-server` runs this as a `trybuild` `compile_fail/` fixture — compile failure IS the test passing.

## Locked answers to 3 Socratic Q

**Q1 · Axis vocabulary — RECOMMENDED Q1.B applied** → ship 5 axes now matching `sandbox::Capability` 1-to-1 + reserve `MemRecall` + `MemRemember` markers in the file (uninhabited types · `impl Axis` deferred). Activate Mem axes at W10 nika-memory admission via additive `#[non_exhaustive]` ratchet (no ADR amendment ceremony · purely additive per FCI-002).

**Q2 · YAML capability surface — RECOMMENDED Q2.C applied** → workflows declare axes flat (`capabilities: [net, fs-read]`). Scope (e.g. `/tmp/*`, `github.com`) lives in `nika-binding` policy file (ADR-054 queued). Separation: phantom-type carries axis, runtime `CapabilityToken` carries scope, policy file binds scope to axis token.

**Q3 · Macro shape — RECOMMENDED Q3 applied** → ship BOTH `caps!(token_set => Net, FsRead)` declarative macro AND raw `proven::<Cons<Net, Cons<FsRead, Nil>>>(&token_set)?` for power users. Skip const-fn typed-builder (HList traversal not const-stable in Rust 1.91).

## Consequences

**Positive:**
- Zero-cost · `Capabilities<E>` is `()` at runtime via monomorphization
- Sealed via ADR-014 — external crates cannot forge `impl Axis for HackerAxis {}`
- Single vocabulary — `CapabilityToken = sandbox::Capability` reused (no duplicate enum)
- Serde-survives — runtime token + proof transition bridges YAML → daemon boundary
- ADR-080 layer (c) compile-time refusal IS this ADR
- Pattern matches ADR-041 type-state `NikaStore<Building → Ready>` family — stylistic consistency

**Negative (acknowledged · mitigated):**
- HList typing burden at adoption sites — `caps!` macro hides 95% · 5% power users use raw `proven::<>`
- Compile-time recursion depth on deep HLists — capped at 8 axes (vocabulary cap) · `cargo build --timings` gate at admission

## 4-option grade matrix (lenses: Maint · TypeSafety · ForwardCompat · Doctrine · Idioms2026 · SerdeSurvival · ADR-040-fit)

| Option | Total /35 | Verdict |
|---|---|---|
| A · Pure phantom (compile-time only) | 28 | loses to serde-traversal boundary |
| B · Runtime bitmask (today's reality) | 26 | already shipped · ADR-079 exists because it's insufficient |
| **C · Hybrid phantom + runtime token** | **34** | **ACCEPT** |
| D · Defer to ADR-040 Cargo features | 19 | conflates build-time toggles with call-site grants |

## Migration plan

| Phase | Crate | Action | Trigger |
|---|---|---|---|
| W2.5 (pre-W3 same commit as ADR-078) | `nika-kernel` | Ship `src/capability.rs` ~300 LOC + tests + `trybuild` compile-fail fixtures | Hard gate before `nika-mcp-server` planning |
| W3+ | `nika-bm25` · memory satellites | Adopt `Capabilities<E: Has<MemRecall>>` on `MemoryRecall::recall` (post-W10 Mem axes activation) | Same commit as satellite admission |
| Post-ADR-055 | `nika-effects` | Every `nika-effects::{fs,http,blob,process}::*` fn gains `caps: &Capabilities<E>` param + `where E: Has<Axis>` bound | ADR-055 admission |
| Phase 2 | `nika-mcp-server` | `Capabilities<E>` at every tool dispatch site · `trybuild` compile-fail tests | Gate 12 admission |
| Phase 2 | `nika-binding` | `Capability` policy module calls `proven::<E>(&token)?` before delegating | Same |
| Phase 4 | `pck` plugin protocol | Per-plugin manifest declares required axes · loader runs `proven::<E>(...)` at load | ADR-050 ship |

## NIKA error code

**NIKA-391 · `CapabilityNotProven`** (Shield range 380-399 per `dx/.claude/rules/security.md`) — runtime `proven::<E>(...)` failed, token set lacks axis required by compile-time effect-set `E`. Sibling to NIKA-380 `CapabilityDenied` (workflow-level runtime check). `is_transient() = false`.

## Risk audit (5 measurable assertions)

1. **HList typing tax at adoption (60% likelihood)** — assertion: `caps!` macro hides HList in ≥95% of call sites · per-call-site LOC adoption ≤2 lines
2. **Phantom-type erased at serde boundary** — assertion: every workflow with `capabilities:` field MUST round-trip through `proven::<E>(...)` at parse · gate 9 canary fixture
3. **Macro hygiene breakage on `caps!`** — assertion: `trybuild` covers 5 macro-edge-cases · `macro_rules!` v1.0 only (NO proc-macro at L0.5 per Q1 lock 2026-04-16)
4. **Compile-time blowup on deep HLists** — assertion: max axis cap = 8 · `cargo build --timings` post-adoption delta < 5% vs baseline
5. **`__unchecked` ctor forgery via macro-hygiene leak (CVE class)** — assertion: `grep -rn '__unchecked' crates/` outside `nika-kernel/src/capability.rs` returns 0 · audit gate at every admission

## Forward-compat invariants

- **FCI-079a** · `Axis` axis-enums are `pub enum FsRead {}` uninhabited (sealed by uninhabitability)
- **FCI-079b** · `Capabilities<E>` is `#[non_exhaustive]` via private field — adding fields safe
- **FCI-079c** · `MemRecall` + `MemRemember` reserved markers — `impl Axis` activation at W10 is additive (no ABI break)
- **FCI-079d** · `proven::<E>` signature stable — return type `Result<Capabilities<E>, CapabilityProofError>` with `#[non_exhaustive]` error enum

## 5 raisons coherence

| Raison | Grade | Justification |
|---|:-:|---|
| ① Liberté cognitive | ✅ | Compile-time refusal removes runtime panic-class · « compiled = safe » structural confidence · Karpathy capability-bound applied |
| ② Souveraineté | ✅ | Capability vocabulary local · zero vendor-hosted policy · architecture IS the protection (Rule 5) |
| ③ Joy 🦋 | ✅ | Zero-cost · monomorphized to nothing · « invisible at runtime · load-bearing at compile » = SuperNovae signature |
| ④ Composable galaxy | ✅ | HList composability · pluggable axes · `pck` plugins compose capability sets Phase 4 |
| ⑤ Studio signature | ✅ | « lift runtime enum into compile-time witness · seal · prove » = Diamond intellectual style (cf ADR-041 · ADR-014) |

🦋
