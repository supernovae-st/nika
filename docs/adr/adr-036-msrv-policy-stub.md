---
id: FCI-036
title: "MSRV policy: rust-toolchain.toml SSOT + N-2 stable cadence (stub)"
status: proposed
date: "2026-04-17"
phase: "Phase D — Wave 4E #2"
deciders: ["@ThibautMelen"]
tags: ["msrv", "toolchain", "forward-compat", "ci", "hygiene", "reservation"]
affects_crates: ["*"]
affects_layers: ["L0", "L0.5", "L1", "L2", "L3", "L4", "L5"]
supersedes: []
superseded_by: []
related: ["ADR-025", "ADR-028", "ADR-022"]
requires: ["ADR-028"]
enables: []
amends: []
fci: ["FCI-036"]
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.81.0-alpha.N, Wave 4E #2 seed (prose pending Phase C)"
follow_ups:
  - "Flesh out full rationale + alternatives during Phase C ADR prose sweep"
  - "Choose MSRV cadence formula (N-2 stable vs fixed version + bump PRs)"
  - "Decide CI matrix: pinned MSRV job vs stable-only with cargo-msrv verify"
  - "Codify interaction with publish = false foundation crates (ADR-022)"
---

# FCI-036: MSRV policy: rust-toolchain.toml SSOT + N-2 stable cadence (stub)

> **STUB — prose pending Phase C.** Decision is load-bearing in code
> (`rust-toolchain.toml` already pins the toolchain; CI bacon rebases depend on
> it); this file exists so vector 19 (adr-orphan-proposed) can surface it at
> day 31 and force full prose authoring.

## Context

Nika Diamond is forever-v0.x (ADR-002). Every foundation crate carries
`publish = false` (ADR-022). The workspace pins a single exact Rust
toolchain via `rust-toolchain.toml` today, with no declared MSRV (Minimum
Supported Rust Version) separate from the pinned toolchain.

Three signals force the MSRV conversation now, not at v1.0:

1. **`nika-sdk` will ship to crates.io** (ADR-022 consequences) — the moment
   a crate has `publish = true`, downstream consumers need an MSRV promise
   to constrain their own toolchain matrix.
2. **Phase C admission velocity picks up** — 5 → 42 crates over 11-12
   months. Without a stated policy, every admission is a one-off MSRV
   negotiation.
3. **The Rust ecosystem standard is explicit MSRV** — Cargo's
   `package.rust-version` field, `cargo msrv verify`, and the MSRV-aware
   resolver (`resolver = "3"`, Rust 1.84+) all assume the project declares
   an MSRV that is **separate from** the developer toolchain.

Related ADRs + signals:

- ADR-025 (per-crate semver via release-plz) — the release pipeline that
  will eventually publish `nika-sdk` + `nika` binary.
- ADR-028 (forward-compat reservation policy) — the forever-v0.x
  invariant that MSRV is the user-facing compatibility dial for toolchain
  churn.
- FCI-008 (public API discipline via CI) — MSRV is a subset of public
  API: raising MSRV is a breaking change in the same sense as removing a
  trait method.
- Hygiene vector #24 (crate-size 15k) and vector #27 (cargo-deny
  duplicates) — both rely on the pinned toolchain reproducing CI
  locally.

## Decision (preliminary)

**Source of truth**: `rust-toolchain.toml` at the workspace root pins the
exact toolchain used by CI + developers (channel + components + targets).
This is the build-time toolchain.

**Declared MSRV**: Each publishable crate (`publish = true`) carries a
`package.rust-version = "1.X"` in its `Cargo.toml`. Foundation crates
(`publish = false`, per ADR-022) do **not** carry `rust-version` — they
inherit the workspace pinned toolchain from `rust-toolchain.toml`.

**Cadence (preliminary)**: MSRV tracks `stable - 2` at minor-version bump
time. When v0.81 is tagged, MSRV becomes the stable release that shipped
roughly 12 weeks prior. No MSRV bump mid-minor. Bumping MSRV is a
minor-version tick for any published crate.

**CI coverage (preliminary)**:

- Primary CI matrix runs on `rust-toolchain.toml`'s pinned channel
  (stable).
- A secondary `msrv-verify` job runs `cargo msrv verify --workspace` on
  every PR for every crate with `publish = true`. Failure is a hard
  block.
- Foundation crates (`publish = false`) are **not** MSRV-verified — CI
  green on the pinned toolchain is sufficient.

**Relationship to `publish = false`**: foundation crates deliberately
opt out of the MSRV contract. This is the trade-off that funds the
rename/refactor velocity ADR-022 locks in. Once a crate flips to
`publish = true` (earliest: `nika-sdk` at L4 admission, per bottom-up progression ADR-037), it inherits the full
MSRV discipline and loses the rename freedom.

**Toolchain components**: the pinned toolchain MUST include `rustfmt`,
`clippy`, `rust-src`, `rustc-dev` (for dylint lint plugins). Targets
include `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`,
`x86_64-apple-darwin`, `x86_64-pc-windows-msvc`.

## Open questions (Phase C prose must resolve)

1. **Cadence formula — N-2 stable vs fixed version + bump PR?** Serde
   uses fixed with bumps. Tokio uses rolling N-5. Axum uses rolling N-3.
   Forever-v0.x argues for a smooth rolling policy; packaging argues for
   predictable fixed targets.
2. **MSRV resolver behaviour**: do we opt into `resolver = "3"`
   (Rust 1.84+ MSRV-aware) the moment a published crate exists? If so,
   the workspace pin must be ≥1.84, and `nika-sdk`'s MSRV is clamped.
3. **Does the `rust-toolchain.toml` pin get bumped on every minor stable
   release, or held steady until the MSRV target window moves?** Today
   the file is bumped opportunistically; that is incompatible with any
   meaningful MSRV promise.
4. **Nightly-only features**: dylint (ADR-011 reserves `nika-lints`) and
   `cargo +nightly miri` in the test CI are nightly-bound. Do we pin a
   secondary nightly channel in `rust-toolchain.toml` or keep nightly
   use confined to xtask scripts that manage their own toolchain?
5. **Backport policy**: if a v0.81.x patch fixes a v0.81.0-published
   crate, does it bump MSRV? Convention says no; rolling N-2 may force a
   bump. Resolve during Phase C.
6. **Interaction with `[workspace.package.rust-version]`**: do we set
   workspace-level rust-version and let crates inherit via
   `.workspace = true`, or per-crate? The first is simpler; the second
   allows crate-by-crate MSRV drift.

## Consequences (preliminary)

- ✅ Foundation crates keep `publish = false` agility; no MSRV tax on
  Phase D refactors.
- ✅ `nika-sdk` (and future published crates) get a clear MSRV promise
  for their downstream consumers.
- ✅ CI bifurcates cleanly: pinned toolchain for internals, MSRV
  verification for externals.
- ❌ Developers must remember that `rust-toolchain.toml` ≠ MSRV —
  training cost on every new contributor.
- ❌ `cargo msrv verify` job adds ~2-4 min to PR CI for each published
  crate. Mitigated by running only on `publish = true` crates (≤2 crates
  pre-L4 interface admission).
- ⚠️ MSRV-aware resolver (`resolver = "3"`) requires workspace pin ≥1.84.
  Not adopted until ADR-022 successor amends.

## Alternatives considered (to expand in Phase C)

- **No MSRV policy; pinned toolchain only** — how Nika works today. Works
  fine while `publish = false` applies universally; breaks the moment
  `nika-sdk` ships.
- **Rolling stable (no MSRV)** — every published crate compiles on the
  latest stable only. Incompatible with the Rust ecosystem's MSRV-aware
  resolver; community blowback guaranteed.
- **Per-crate MSRV drift** — every crate picks its own MSRV.
  Administrative overhead outweighs flexibility for a 40-42-crate
  workspace.
- **Workspace MSRV inherited via `workspace = true`** — simpler, locks
  all published crates to one MSRV. Preferred under Phase C decision.

## See also

- ADR-022 — Foundation crate layout v0.81 (locks `publish = false` for
  foundation crates).
- ADR-025 — Per-crate semver via release-plz (the pipeline that enforces
  MSRV bumps).
- ADR-028 — Forward-compat reservation policy (this ADR is an instance
  of "reserve the policy shape before implementation lands").
- FCI-007 — Feature flags with stable defaults (MSRV is the "stable
  default" for toolchain).
- FCI-008 — Public API discipline via CI (MSRV is a subset of the public
  API contract).
- `rust-toolchain.toml` at workspace root — current pinned toolchain
  (source of truth for developer + CI build toolchain).
- `Cargo.toml` `[workspace.package]` — future home of
  `rust-version` field once the cadence is decided.

🦋
