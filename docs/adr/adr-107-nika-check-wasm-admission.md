---
id: ADR-107
title: "Admit nika-check-wasm — the static half of nika check, compiled to the browser"
status: proposed
date: "2026-07-28"
phase: "post-0.106 · the compiler-wedge arc (nika.sh /play oracle)"
deciders: ["@ThibautMelen"]
tags: ["wasm", "check", "L4", "conformance", "browser", "diamond", "admission"]
affects_crates: ["nika-check-wasm", "nika-schema", "nika-check"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-020", "ADR-032", "ADR-038"]
amends: []
requires: ["ADR-003"]
enables: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "built 2026-07-28 on feat/check-wasm · admission at the operator's ceremony"
follow_ups:
  - "Extract the remaining static legs (COST…TRIFECTA) from nika-cli into a shared seam — never re-assemble them here (leg B is the tripwire)"
  - "Feature-gate jaq/jsonschema out of nika-check for the wasm diet (3.1M → the parse+conform floor)"
  - "Wire leg B (NIKA_DIFF_CLI=1) into CI so the row-equality gate runs on every engine push"
---

# ADR-107: Admit `nika-check-wasm`

## Context

The spec reserved this seat before the crate existed — conformance Level 2
names « custom engines for specialized environments (embedded · **WASM** ·
custom LLM gateway) » (`spec/07-conformance.md`). nika.sh's `/play` judged
files with a 13-code TypeScript approximation of a 96-code checker, under a
site law that forbids hand-written verdicts; the site had even drawn the seam
in advance (`window.NikaOracle`, « the day the engine ships its wasm check
artifact… »). This crate is the seat taken: the same parser, the same
judgment crates, the same error voice, for the legs a browser genuinely has —
and its coverage stated in-band (`wasm: true` + closed `legs: []`) so the
browser half can never claim the binary's reach.

The artifact already ships, honestly labelled, on nika.sh `/play`
(vendored pkg + PROVENANCE.json naming this branch, pre-admission — a
site decision, taken because the alternative in production was strictly
worse). Admission regularises the seat, it does not create the exposure.

## Decision

Admit `nika-check-wasm` as an L4 transport surface (the `nika-lsp`/`nika-mcp`
seat: it exposes, it never judges), moving it out of `wip = [...]`.

## Gate-by-gate readiness (evidence, not claims)

| # | Gate | State | Evidence |
|---|------|-------|----------|
| 1 | SPEC doc | ✅ | `docs/crate-specs/nika-check-wasm.md` (LOC live-anchored 234) |
| 2 | TDD red/green | ✅ with an honest note | the column test refuted its author twice before it held (wrong line, then under the anchor — three probes to find the flow-mapping shape); leg B went red on a wrong bin-target before green. Written test-first is not claimed for the v0 scaffold. |
| 3 | IMPL compiles | ✅ | native + `wasm32-unknown-unknown` (`--profile wasm-release`), workspace green |
| 4 | Zero clippy | ✅ | `cargo clippy -p nika-check-wasm --all-targets` clean (pedantic workspace lints) |
| 5 | Mutation ≥90% | ✅ 95.8% | 24 mutants: 22 caught + 1 timeout (the `-=`→`/=` infinite loop — detected by hanging) + 1 EQUIVALENT (`>`→`>=` on `at > 0`: at 0, `is_char_boundary(0)` is always true, the `&&` exits identically — indistinguishable by construction). 100% of non-equivalent mutants killed. |
| 6 | Property tests | ✅ | `line_col` under arbitrary unicode + arbitrary offsets vs an independent char-walking reference (mid-char, past-end, empty, newline-at-offset) |
| 7 | Benchmarks | n/a | the hot paths are `parse` + `analyze`, owned and measured by `nika-schema`/`nika-check`; this crate adds one O(offset) arithmetic per spanned finding |
| 8 | Zero doc warnings | ✅ | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p nika-check-wasm` clean |
| 9 | Canary E2E | ✅ (consumer-side) | nika.sh `check-wasm-oracle.test.ts` loads the real artifact in node and re-judges the SERVED hero twins against the CLI-captured truth |
| 10 | Golden parity | ✅ | the differential pair: leg A (assembly vs library, 125-fixture corpus, always) · leg B (rows vs same-tree CLI, `NIKA_DIFF_CLI=1`) — both seen red before green |
| 11 | 3-agent swarm | ⏳ | run at prep (spn-nika reviewer · rust-security · correctness) — findings + resolutions appended below |
| 12 | Atomic commit | ⏳ | the ceremony itself: squash-merge of `feat/check-wasm` with this ADR flipped to `accepted` |

## Consequences

- The one-voice law crosses the network: a finding on nika.sh IS a
  `SchemaError` rendered by the same `Display` and `spec_code()` as the
  binary — no port, no paraphrase, no drift surface.
- The remaining-legs seam becomes load-bearing debt: any future leg lands by
  EXTRACTION from `nika-cli`, never re-assembly here (the check↔run oracle's
  divergence class; leg B is the standing tripwire).
- The wasm diet (jaq/jsonschema linked into a parse+conform artifact) is
  named, owned, and deferred — 3.1M raw today, lazy-loaded on one route.

## Gate 5 · mutation run (2026-07-28)

`cargo mutants -p nika-check-wasm` · 24 mutants in 63s · **22 caught · 1
timeout · 1 missed**.

- The timeout IS a detection: `at -= 1` → `at /= 1` never decreases the
  cursor, the boundary walk spins forever, the suite hangs — a mutant that
  can only be observed by not terminating.
- The miss is an **equivalent mutant**: `at > 0` → `at >= 0`. At `at == 0`,
  `is_char_boundary(0)` is true for every string, so the `&&` short-circuit
  exits the loop under both spellings; no test can distinguish them because
  no behaviour does. Documented here rather than contorted around.
- The first run also missed both `engine_version` mutants
  (`String::new()` · `"xyzzy"`); consumers pin that value against captured
  provenance, so the kill test asserts it equals `CARGO_PKG_VERSION`.

## Gate 11 · swarm findings and resolutions

(appended at prep close)
