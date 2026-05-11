---
id: ADR-024
title: "Adopt SOTA Rust patterns: bon Builder, Arc<str>, camino, sealed module, strum, public-api"
status: accepted
date: "2026-04-16"
phase: "Phase H — SOTA patterns adoption"
deciders: ["@ThibautMelen"]
tags: ["rust-idioms", "sota", "bon", "builder", "arc-str", "camino", "sealed", "strum", "public-api"]
affects_crates: ["nika-core", "nika-error", "nika-kernel", "nika-catalog", "nika-event", "nika-schema-ast", "all"]
affects_layers: ["L0", "L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-021", "ADR-022", "ADR-027", "ADR-028"]
requires: ["ADR-022"]
enables: []
amends: []
fci: []
inv: ["INV-019"]
shadow_zones: []
nika_codes: []
timeline: "v0.81+"
follow_ups: ["per-pattern migration commits in Phase H"]
---

# ADR-024: Adopt SOTA Rust patterns

## Context

Multi-agent audit (rust-architect + rust-pro + web-researcher) identified six
2025-2026 SOTA Rust patterns that Nika can adopt cheaply at v0.1 with high
forward-compat ROI.

## Decision

### 1. `bon::Builder` derive as DEFAULT for ≥4-arg structs

Adopters: rust-analyzer, axum, tokio (evaluating). Type-state safety,
`#[non_exhaustive]`-compatible, eliminates `..Default::default()` tax.

Apply to top 5 first: `InferRequest`, `AgentLoopConfig`, `ModelCapabilities`,
`HttpRequest`, `ScheduleConfig`. Inv #19 amended: `new()` for ≤3-arg structs,
`bon::Builder` for ≥4-arg.

### 2. `Arc<str>` for shared catalog identifiers

`ModelId`, `ProviderId`, capability rule keys: migrate from `String` to
`Arc<str>`. Half the size, O(1) clone (refcount). Static catalog literals stay
as `&'static str` (phf keys unchanged).

### 3. `camino::Utf8PathBuf` for ALL paths in nika-core

Kills the `OsStr → str` unwrap question forever. Matches Cargo, jj, oxc, ruff,
uv convention.

### 4. `mod sealed { pub trait Sealed {} }` as the ONLY sealed pattern

Per Agent 2 SOTA: line-for-line identical pattern across rust-analyzer, biome,
ruff, bevy, tokio, oxc. Drop the proposed `sealed` proc-macro crate (3-project
trend, not convergence).

### 5. `strum::EnumString + IntoStaticStr` for enum↔string bridges

Specifically for `EventKind` aggregation in nika-event (22 sub-enums). Used by
oxc (200+ AstKind variants), bevy (Reflect), sqlx (error codes). Q1 still
holds (no proc macros in *L0 impl*); strum is a dev-dep proc macro, not Nika-
owned L0 code.

### 6. `cargo public-api` baselines NOW for L0 + L0.5

Snapshot every public symbol of every L0/L0.5 crate into
`docs/api-baselines/<crate>.txt`. Pre-PR hook diffs against baseline. Mechanizes
the `#[non_exhaustive]` ratchet (vector 18 made enforceable).

## Consequences

- ✅ Type-state builder safety (#1) prevents misconstructed configs at compile
  time
- ✅ `Arc<str>` (#2) — half the alloc per catalog lookup
- ✅ `camino` (#3) — kills a class of unwrap bugs
- ✅ Plain `mod sealed` (#4) — minimal complexity, industry standard
- ✅ `strum` (#5) — eliminates handwritten `as_str()`/`parse()` boilerplate
- ✅ `public-api` (#6) — automated #[non_exhaustive] ratchet
- ❌ +4 workspace deps: `bon`, `camino`, `strum`, `cargo-public-api` (cargo
  subcommand, not a runtime dep)
- ❌ ~8 commits of mechanical migration work across Phase H

## Migration order

1. `cargo public-api` setup + baselines (no code changes — PR-blocking
   infrastructure)
2. `mod sealed` standardization (already mostly there)
3. `camino::Utf8PathBuf` migration (mechanical find/replace)
4. `Arc<str>` for ModelId/ProviderId (touches catalog; coordinated with
   ADR-022 split)
5. `strum` for EventKind in nika-event (during nika-event build)
6. `bon::Builder` for top 5 config structs (last — depends on stable APIs)

## Alternatives considered

- **`derive_builder`** instead of `bon` — bon's #[non_exhaustive] support is the
  tipping point
- **`SmolStr` / `CompactString`** instead of `Arc<str>` — defer until cargo-bench
  justifies (small-string optimization is hot-path-specific)
- **`std::path::PathBuf`** instead of `camino::Utf8PathBuf` — keeps OsStr→str
  unwrap question
- **Custom `as_str()` on each enum** instead of `strum` — boilerplate at scale
