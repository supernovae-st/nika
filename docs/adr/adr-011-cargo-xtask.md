---
id: ADR-011
title: "cargo xtask as canonical automation surface"
status: accepted
date: "2026-04-14"
phase: "Phase 2 (migration window)"
deciders: ["@ThibautMelen"]
tags: ["tooling", "xtask", "automation", "ci"]
affects_crates: []
affects_layers: []
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-004", "ADR-009"]
requires: ["ADR-003"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.80.0, Phase 2 spec -- migration before Phase 3"
follow_ups: ["Migrate scripts/ci/*.sh to xtask subcommands"]
---

# ADR-011: `cargo xtask` as canonical automation surface

## Context

Diamond currently runs CI ratchets + hygiene checks via ~10 bash scripts under `scripts/ci/` and supernovae-hq parent runs 10 more under `scripts/hygiene/`. Bash works for 5 admitted crates. It does not scale to 40 — concerns:

- **Portability:** macOS bash 3.2 quirks already bit us (associative arrays). Windows users of `nika init` cannot run these scripts at all.
- **Type safety:** parsing `Cargo.toml` via `awk` + `grep` is fragile; we've already fixed one regex-injection bug (supernovae-hq allowlist checker).
- **Composability:** rust-analyzer, Zed, tokio, rust-lang/rust itself all migrated to `cargo xtask` pattern (separate workspace crate named `xtask`, unaffiliated with release binaries).

Web researcher and Nika-SOTA architect both flagged this as the #2 impact-per-effort improvement.

## Decision

Introduce an `xtask/` workspace crate (type `bin`) that owns all Diamond automation:

- `cargo xtask gate <crate>` — run 12 gates against one crate
- `cargo xtask hygiene` — the 15-vector-adjacent parent hygiene
- `cargo xtask loc` / `xtask crate-size` / `xtask fn-length` — ratchet checks (replacing `scripts/ci/check-*.sh`)
- `cargo xtask bench` — orchestrate criterion + divan runs
- `cargo xtask release <crate> <version>` — tag + CHANGELOG sync

Shell scripts stay as **thin shims** calling `cargo xtask <subcmd>` until fully retired. `xtask` crate is excluded from the 100-crate cap (industry convention — xtask is not a library crate).

## Consequences

### Positive
- Cross-platform (Windows for `nika init` users).
- `cargo_metadata` crate for manifest parsing — typed, no regex.
- Subcommand composition (can call other xtask subcmds programmatically).
- Unit-testable automation logic (bash is not).

### Negative
- New crate adds compile time (cold build ~15s extra one-time).
- Migration is ~2 sessions of focused work for the 20+ scripts.

### Neutral
- Convention-only exclusion from crate cap. Documented in ADR-004.

## Evidence

- Pattern source: [rust-lang/rust x tool](https://github.com/rust-lang/rust/tree/master/src/tools/x), rust-analyzer `xtask`, tokio `xtask` (upstream examples, not local paths)
- `scripts/ci/*.sh` — 10 scripts currently
- `scripts/hygiene/*.sh` (in supernovae-hq parent) — 11 scripts

## Alternatives considered

### Alt A — Keep bash
Rejected — portability + type safety.

### Alt B — Python scripts
Rejected — adds Python runtime to CI. We're a Rust workspace.

### Alt C — `just` (justfile)
Considered. Lower friction than xtask but still not-Rust. Loses `cargo_metadata` access. xtask is idiomatic for our scale.

## Related

- ADR-003 — 12-gate admission (xtask `gate` subcommand hosts the gates)
- ADR-004 — context-window sizing (xtask is the documented exclusion from the 100-crate cap; this ADR formalises the convention ADR-004 references)
- ADR-009 — ADR process (`check-adr-coverage.sh` → `xtask check-adr-coverage`)

## Notes

Migration is non-urgent. Execute before Phase 3 admissions (when verb crates start landing and gate complexity grows). Until then bash shims remain authoritative.
