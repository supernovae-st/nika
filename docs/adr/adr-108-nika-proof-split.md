---
id: ADR-108
title: "Split the spec-15 proof primitives out of nika-runtime into nika-proof (L0)"
status: accepted
date: "2026-07-29"
phase: "post-0.106 · the F-P6 preview→commit arc"
deciders: ["@ThibautMelen"]
tags: ["proof", "receipt", "L0", "crate-size", "diamond", "split"]
affects_crates: ["nika-proof", "nika-runtime", "nika-dap"]
affects_layers: ["L0", "L3"]
supersedes: []
superseded_by: []
related: ["ADR-022", "ADR-090"]
amends: []
requires: []
enables: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "built 2026-07-29 on feat/f-p6-preview-commit · accepted 2026-07-29 by the operator (Gate 12 = the squash-merge that lands this file)"
follow_ups:
  - "Admit nika-proof through the 12-gate Diamond table (it lands WIP) when a lane touches the receipt surface next"
  - "Move the resume family's task-definition projection beside the ir projection if resume.rs ever crosses its own wall"
---

# ADR-108: Split the spec-15 proof primitives into `nika-proof`

## Context

The 15k production-LOC invariant (`scripts/ci/check-crate-size.sh` ·
CONSTELLATION_PLAN §7 criterion 3) is a hard pre-push gate, and nika-runtime
sat at 14,952 of 15,000 on `main` (`bee82f9b1` · 99.7% full). The F-P6
preview→commit lane adds ~460 LOC of new machinery (the canonical request,
the digest gate, the evidence plumbing) that no amount of documentation diet
could fit under the cap — measured with the gate's own counter
(`git ls-files` · src/ minus `#[cfg(test)]` minus `tests.rs`).

The house's documented answer to a full crate is a descent split — the
workspace `Cargo.toml` layer comments name the precedents verbatim
(`nika-dap` split from nika-cli "the crate-size cap · the nika-cap
precedent", `nika-models` the same).

## Decision

Create **`nika-proof`** (L0, WIP) carrying the spec-15 proof layer's pure
hash primitives — the seven `HashDomain`s, the JCS canonical bytes, the
domain-separated pre-image, the blake3 semantic hash, and the one receipt
(`build_receipt` · `build_run_receipt` · `verify`) — moved verbatim from
`crates/nika-runtime/src/proof/{mod,receipt}.rs`.

The `ir` projection (`semantic_ir_hash` · `task_semantic_hash`) STAYS in
nika-runtime: it reuses the resume family's task-definition projection
(`crate::resume::definition_value`), so it cannot descend without dragging
the family with it. nika-runtime re-exports the whole crate at the exact
old path — `pub mod proof { pub use nika_proof::*; pub mod ir; }` — so
every consumer path (`nika_runtime::proof::{preimage, HashDomain,
receipt::build_receipt, …}`) resolves unchanged and nika-dap needs no edit.

Rejected: shrinking the F-P6 machinery to fit (the lane's honest minimum
exceeds the 48 free lines by ~10×); moving the judge to nika-cap (covers
~120 of ~460 · still red); an admitted-crate birth (the 12-gate ceremony is
its own lane — the crate lands WIP, the `wip` array's own posture).

## Consequences

### Positive

- nika-runtime drops to ~14.4k prod LOC — the F-P6 lane fits, and the next
  lanes get ~600 lines of headroom back.
- The receipt/hash primitives become consumable without the whole L3
  runtime (nika-dap already reads them through the re-export).
- The split is history-clean (`git mv` — both files keep their blame).

### Negative

- The proof layer now lives in two places by necessity (`ir` in
  nika-runtime · the primitives in nika-proof) — the follow-up names the
  reunification condition (the resume projection descending too).
- `nika-runtime/public-api.txt` re-renders the moved items as the glob
  re-export (paths preserved · additive under cargo-semver-checks).
- A new WIP crate to admit later (the 12-gate table).

### Neutral

- The layer registry (`Cargo.toml` `[workspace.metadata.diamond]` +
  `docs/architecture/crate-layer-registry.md`) gains the L0 row; the
  status-claims projection (`scripts/refresh-status.sh` · vector 23) is
  regenerated (58 crates · 3 WIP · 19 L0).

## Evidence / Affected code

- `crates/nika-proof/src/lib.rs` — ex `crates/nika-runtime/src/proof/mod.rs`.
- `crates/nika-proof/src/receipt.rs` — ex `crates/nika-runtime/src/proof/receipt.rs`.
- `crates/nika-runtime/src/lib.rs` — the `pub mod proof { pub use nika_proof::*; pub mod ir; }` seam.
- `crates/nika-runtime/src/proof/ir.rs` — stays (the resume-projection coupling).
- `Cargo.toml` — `members` + `wip` + `layers.nika-proof = "L0"`.

## Alternatives considered

1. **Trim the lane to the cap** — measured: the design-minimal machinery is
   ~260 LOC against 48 free lines. Rejected as impossible without dropping
   design laws.
2. **nika-cap as the judge's home** — covers the pure judge (~120 LOC) but
   leaves the dispatch lanes + settle plumbing in a still-red crate.
   Rejected as insufficient.
3. **Split `compose.rs`** — larger (1,326 LOC) but the public composer API
   is the workspace's hottest re-export surface. Rejected as the riskier
   move; the proof primitives are the cleaner descent.
