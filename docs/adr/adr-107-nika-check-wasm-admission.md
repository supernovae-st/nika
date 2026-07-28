---
id: ADR-107
title: "Admit nika-check-wasm — the static half of nika check, compiled to the browser"
status: accepted
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
timeline: "built 2026-07-28 on feat/check-wasm · accepted 2026-07-28 by the operator (Gate 12 = the squash-merge that lands this file)"
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
| 11 | 3-agent swarm | ✅ | three legs returned, every finding resolved or declined-with-rationale below — including one BLOCKING engine bug (the jq nesting bomb) the corpus could never have found |
| 12 | Atomic commit | ✅ | this squash-merge — the ADR flipped to `accepted` on the operator's explicit word, 2026-07-28 |

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

### Leg 1 · spn-nika reviewer (one-voice · zero-unwrap · I/O · layers)

Verdict: hard rules ALL PASS (one-voice: only `spec_code()` + `Display` +
`ERROR_DOCS_BASE` reach the wire, `.nika_code()` never called · zero unwrap
outside `cfg(test)` · zero I/O so the kernel seam is correctly vacuous · L4
defensible under the registry's own sort test). Changes requested on
PROCESS, and the reviewer was right:

| Finding | Severity | Resolution |
|---|---|---|
| **No CI job ever built wasm32** — the crate's own reason to exist was verified only by hand | Critical | **FIXED at prep** · `scripts/ci/check-wasm.sh` + the `wasm` matrix leg in diamond-ci.yml: clippy `-D warnings` on the target, `--profile wasm-release` build, and differential Leg B |
| Leg B (`NIKA_DIFF_CLI=1`) wired nowhere — silently no-ops unless a human remembers | Warning | **FIXED at prep** · the same CI leg exports it; the spec-fixtures checkout step now serves `tests` and `wasm` legs both |
| No `docs/crate-specs/nika-check-wasm.md` | Warning | **already landed in parallel** during prep (the reviewer read the pre-edit tree) — live-anchored at 234 LOC |
| `wasm-bindgen`/`getrandom`/`getrandom-02` pinned crate-side against the pin-once workspace rule | Warning | **FIXED at prep** · all three centralised in `[workspace.dependencies]` with attribution comments; the crate goes `.workspace = true` |
| `crate-layer-registry.md` L4 rows not updated — and the two PRE-EXISTING near-duplicate L4 table rows (166/167, differing only by `nika-onboard` vs `nika-models`) | Warning | **FIXED at prep** · the duplicate rows merged into one honest row carrying both crates, `nika-check-wasm` seated in the row and the ASCII map (marked WIP · ADR-107) |
| README owed-list numbering skipped an item | Suggestion | **FIXED at prep** |
| The span arithmetic deserves a proptest | Suggestion | **already landed in parallel** — `line_col` vs an independent char-walker under arbitrary unicode (Gate 6) |
| A typed verdict (`tsify`/hand `.d.ts`) at the TS boundary | Suggestion | **DECLINED with rationale** · the verdict's contract is CLI parity, held by the differential pair and the site's node gate; a second typed assembly would be a second thing able to drift — the exact divergence class Leg B exists to kill. The consumer types what it consumes (`oracle.ts` `WasmRow`), and the gate proves it against the artifact. |
| L4 carries no `layer-bans` entry | Note | **HELD for the ceremony** · what L4 may never depend on is a policy decision, the operator's; the CI wasm leg already provides the practical stop the finding wanted (a wasm32-incompatible dep now reddens every push) |

### Leg 2 · rust-security (hostile input · determinism · supply surface)

Method: the reviewer built the real artifact and fed it 125 conformance
fixtures plus ~50 crafted hostile inputs under V8 and a 1 MiB-stack native
harness. PASSES earned, not assumed: the char walk survived every probe
(mid-char, past-EOF, BOM, NUL, emoji — `badspan=0` across the corpus) ·
billion-laughs closed · determinism structural (BTreeMap throughout, 25
processes → 1 output hash, zero entropy consumed on the check path) · SSRF
closed at the schema compiler (`default-features = false`, no fetch). Then
the verdict that mattered: *« nothing in the 12 gates proves panic-freedom
on hostile input — F1 was found in the first ten minutes of feeding it
garbage. »*

| Finding | Severity | Resolution |
|---|---|---|
| **F1 · a 1,047-byte paste stack-overflows the checker** — jaq's parser recurses per nesting level with no limit; a trapped wasm instance never recovers, and the installed CLI aborts (exit 134) on the same file | **Blocking** | **FIXED in `nika-check`** · pre-flight `jq_nesting_over` guard (string-aware O(n) scan · ceiling 128, the house's own cap, precedents `MAX_VALUE_DEPTH`/`nika-tmpl`) before `jq_compiles`, covering all three doors (`nika:jq` · `nika:fetch` jq · `on_finally`) · regression proves the bomb refuses AND the instance survives · fuzz corpus seeded with the 470-bracket shape |
| **F2 · 28s of synchronous CPU inside the declared caps** — `did_you_mean` is O(n²·L²) across all task ids | High | **FIXED in `nika-check`** · a suggestion budget (256 candidates): past it the DAG-002 finding still fires, it just stops guessing · regression proves finding-without-suggestion at 300 ids · the Worker seat for the site consumer stays a named follow-up |
| F3 · `<` `>` `&` U+2028/9 legal in JSON strings, hostile in every context a site inlines JSON into | Medium | **FIXED** · `escape_for_embedding` rewrites the five to `\uXXXX` over the emitted text (only ever inside string literals → equivalent JSON) · regression proves zero raw occurrences and an intact round-trip |
| F4 · `build-wasm.sh` could package a wasm it did not build (`CARGO_TARGET_DIR` + hardcoded path) · `--lib` skipped the differential · no `--locked` | Low | **FIXED** · artifact path derived from `cargo metadata` · full `--locked` test run |
| F5 · no `--` terminator before the fixture path in the leg-B spawn | Low | **FIXED** |
| F6 · emitted `span` not clamped (only the line/col walk is) | Low | **DECLINED with rationale** · the emitted span must stay byte-equal to the CLI's (leg A asserts it against the library accessor — clamping would manufacture divergence); `line`/`col` are the sanctioned consumer fields, and the reviewer could not produce an out-of-range span across the corpus |
| F7 · `legs:` under-declares (the CONFORM analyzer also compile-checks jq + JSON Schema) | Info | **FIXED** · the nuance stated in the verdict doc comment; the legs list stays gate-named |
| F8 · miette's terminal stack is pure payload in a browser | Info | already the named diet follow-up (feature-gating the judgment hub's heavy corners) |

### Leg 3 · correctness (contract fit · docs honesty)

PASSES earned: the line/col arithmetic verified at every boundary the hand
can reach, and its property test judged « real coverage, not tautological »
· CRLF self-consistency argued from the shared source string · the leg-B
bin-target and fixture-path resolutions verified against the real
filesystem · the consumer's `line ?? 1` fallback matches the port's own
convention.

| Finding | Confidence | Resolution |
|---|---|---|
| README's Build section documented a WORSE recipe than `build-wasm.sh` (wrong profile · wrong path · no wasm-opt) — a contributor copying it from the crate dir gets a file-not-found, or an unstripped artifact | 88 | **FIXED** · the section is now a pointer at the script («a pointer cannot drift»), with the drift named |
| `severity` · `kind` · the `docs_url` separator asserted NOWHERE — three plausible one-token mutants passed every test in the repo | 85 | **FIXED** · leg A asserts all three per row (exact-format `docs_url`); severity joins leg B's comparison tuple |
| `PROVENANCE.json` « missing » beside pkg/ · no `estate.yaml` row for the vendored artifact | 80 | **HALF-STALE, HALF-PAID** · the file exists one level up (`src/lib/check-wasm/PROVENANCE.json`, beside the pkg it describes — the reviewer walked pkg/ only); the REAL half — the site's estate ledger had no row — is paid site-side with this resolution |

