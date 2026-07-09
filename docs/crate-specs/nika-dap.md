# Crate spec — `nika-dap`

| | |
|---|---|
| Status | **ADMITTED 2026-07-09** — Gate 1 authored at the split (a descent, not a greenfield: every line arrived tested from `nika-cli`). |
| Layer | L4 — interface crate (stdio protocol server + the forensic read seams) |
| Design | The trace-forensics plane: the DAP replay debugger (`nika dap`) plus the seams every forensic reader shares — the tolerant NDJSON reader (`recover`), the tamper-evidence chain walk (`chain`), the source-identity hashes (`source_id`), the forensic statistics (`stats` — the Prior honesty ladder + Hyndman-Fan-7 quantiles every learned-truth reader speaks · descended from nika-cli at the 15060-LOC wall, the same session as the crate itself), and since the W0 descent (§5) the forensic half of the trace family — the OTLP projection (`otel`), the reproduce comparison (`reproduce`), the store scan (`store`) and the retention policy (`retention`). One home so the sink that WRITES the chain and every walker that CHECKS it share one genesis tag and one hash primitive. |
| LOC budget | the 15k-prod workspace ratchet governs (≤1,500/file · ≤100/fn as everywhere) — admitted at ~1,450 src incl. in-file tests; the 2026-07-09 W0 trace descent (§5) added the four forensic trace modules (~1.1k prod) with headroom for live DAP sessions intact |
| File cap | ≤1,500 LOC each (max at admission: `replay.rs` ~490) |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace (`0.98.0` at admission) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L4 interface crate, same stance as `nika-cli` |
| Extraction source | `crates/nika-cli/src/verbs/dap/{mod,protocol,replay}.rs` (1,202 LOC · git-mv, history preserved) + `run/source_id.rs` (moved) + `run/resume.rs::recover_events` (moved) + `verbs/trace_verify.rs::{walk, Verdict}` (moved) — `nika-cli` re-exports every seam at its old path (zero call-site churn). **W0 trace descent (2026-07-09 · architecture review v2 §1)**: `verbs/trace_otel.rs` → `otel` · `verbs/trace_reproduce.rs` → `reproduce` · `verbs/trace/{store,retention}.rs` → `store` + `retention` — the compute descends, the render stays (the CLI keeps `export`/`reproduce` file plumbing, the report/line renderers, `fmt_age`/`fmt_bytes` display vocabulary and the `nika run` GC hook as shims). Per D-2026-07-09-N1 the descent is ONE architectural unit in TWO members — this crate spec names the parentage; the unit stays `nika-cli`'s. |
| NIKA codes | **none** — the DAP wire speaks the protocol's own error responses; the forensic seams return typed verdicts/Results (the trace surface stays non-coded, the same stance the trace verbs hold) |

---

## 1. Purpose

`nika-dap` is the **trace-forensics plane**:

1. **`run_stdio()`** — the Debug Adapter Protocol server behind `nika dap`:
   a READ-ONLY replay debugger over a recorded run journal. Breakpoints
   map to task lines, stepping walks task settles, `stepBack` is free
   because the log is total — replay = re-render, NEVER re-execute.
2. **`recover`** — the ONE tolerant NDJSON reader (`--resume` · `trace
   show` · the store scan · the forecast gather · the replayer all fold
   through it): a torn tail keeps its valid prefix, a dead first line
   refuses.
3. **`chain`** — the tamper-evidence walk (`walk(raw) -> Verdict`) and
   the ONE `CHAIN_GENESIS` constant the sink imports to write the same
   chain the walk verifies.
4. **`source_id`** — `sha256_hex` + `lf_normal_form` (a CRLF re-encode
   is not an edit — the 0.96.0 dap-review lesson lives here).

## 2. Why a crate (and why now)

`nika-cli` sat at 14,828/15,000 prod LOC (98.9%) before the forecast
feature landed (+947): the crate-size ratchet blocked every push. The
2026-07-09 gates audit (§3) weighed three options — compacting was
insufficient (−200 for a needed −775+), bumping the cap is forbidden
(it has paid four times), and the dap module was the cleanest cut:
an external protocol surface whose only inbound edge was ONE match arm
in `main.rs`. The compiler then surfaced the real inverse coupling —
five forensic symbols — and the split became the chance to give them
one home (three private `sha256_hex` copies and two `CHAIN_GENESIS`
tags unified). Precedent: `nika-cap` absorbing `builtin_shape` at the
same wall (2026-07-07).

## 3. Public API (the whole surface)

```text
pub fn run_stdio() -> u8
pub mod recover    { RecoveredTrace · RecoverError · recover_events }
pub mod chain      { CHAIN_GENESIS · Verdict · walk }
pub mod source_id  { sha256_hex · lf_normal_form }
pub mod stats      { Prior (#[non_exhaustive]) · BANDS_MIN_N · quantile_h7 · ConformalUpper · conformal_upper }
pub mod otel       { project (journal + chain Verdict → one OTLP/JSON line) }
pub mod reproduce  { Verdict · Row · Report · compare · workflow_of }
pub mod store      { TRACE_DIR · TraceState · TraceMeta · scan · fold_facts }
pub mod retention  { RetentionConfig · Reason · GcReport · plan · newest_per_workflow · collect }
```

Consumers: `nika-cli` (the bin's `Command::Dap` arm + the re-exported
seams). The DAP protocol/replay internals stay private.

## 5. The W0 trace descent (2026-07-09)

`nika-cli` hit the 15k wall a second time the same day (99.8% ·
14,966/15,000 — two open PRs blocked at the push gate). The
architecture review v2 §1 designed the descent: **the forensic half of
the trace family comes home to the forensics plane** — `trace_otel`'s
projection (embedder-useful without the CLI: OTLP export of any
recorded journal), `trace_reproduce`'s comparison taxonomy (the
replayer competency), and the `store` scan + `retention` math. The
render half STAYS cli-side (`trace/mod.rs` readers · `trace manage` ·
the report/line renderers · the `Theme`/display vocabulary) — compute
descends, render stays, the `trace_verify` shim pattern throughout.
Deps stay L0-only (nika-event · nika-types · sha2 · serde/serde_json ·
thiserror — the absorption is L4-legal). Every moved type follows
FCI-002/FCI-016 (`#[non_exhaustive]` + `new()` per invariant #19);
the two cli-side exhaustive `TraceState` matches gained honest
wildcard arms.

## 4. Gates at admission (2026-07-09)

| Gate | Name | Verdict |
|---|---|---|
| 5 | MUTATION | ✅ 98.6% killed (144/146 viable · 142 caught + 2 timeouts · 21 unviable) — survivors: the two `run_stdio` stubs (the seamless stdio composition root) |

- Tests: 34 in-crate — 25 moved WITH their code (chain walk 8 · replay 12 ·
  protocol 3 · recover 1 · source_id 1) + 9 mutation-killers added at
  admission (the documented cap boundaries · every terminal-kind fold arm ·
  defensive-min totality on a wild cursor · outgoing-seq monotony · the
  serve arms a client probes) — `cargo test -p nika-dap --lib`.
- Clippy: 0 warnings (`--all-targets -- -D warnings`, pedantic-clean).
- Mutation (Gate 5): **98.6% killed** (144/146 viable — 142 caught +
  2 timeouts · 21 unviable) at admission, 2026-07-09. The two survivors
  are the `run_stdio -> u8` stubs: the 4-line stdio composition root has
  no injectable seam by design (its body is exercised through `serve`,
  which the in-crate session tests drive over cursors). The first run
  scored 77% — the 9 killer tests above were written at admission to
  close the gap, not waved through (+ 2 line-number pins on the recover
  fold after the post-review re-run).
- Property (Gate 6): N/A as a dedicated suite — the chain walk's
  adversarial cases (torn tail · dropped line · blank renumbering ·
  single-line garbage) are the property set, exercised as units.
- Benchmarks (Gate 7): N/A — protocol server + linear walks, no hot path.
- Canary (Gate 9): N/A — the `nika dap` verb is exercised by the editor
  extension's F5 flow; the CLI e2e suite covers the seams.
- Parity (Gate 10): N/A — no brouillon ancestor (the DAP server was born
  in Diamond, 2026-07-06).
- Review (Gate 11): 3-agent swarm at admission (2026-07-09) on top of the
  moved code's shipped lineage (PR #225 · the 0.96.0 dap review · the
  rust-pro hardening batch). Verdict FIX-THEN-ADMIT, all resolved same
  session: `Verdict` + its struct variants gained `#[non_exhaustive]`
  (+ the wildcard arm in `trace verify`'s render), `RecoveredTrace`
  gained `#[non_exhaustive]` + `new()` (invariant #19), and
  `recover_events` returns a typed `RecoverError` (thiserror · FCI-019 —
  never a bare `String` in public API).
