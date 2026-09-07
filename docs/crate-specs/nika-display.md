# Crate spec — `nika-display`

| | |
|---|---|
| Status | **ADMITTED 2026-07-10** — Gate 1 authored at the split (a descent, not a greenfield: every line arrived tested from `nika-cli`). |
| Layer | L4 — interface crate (the run-comprehension surface · pure event→text) |
| Design | The whole comprehension surface the operator reads: the event fold (`state` — `RunView` as a pure function of the stream), the terminal frames (`render` — the storyboard, the meter, the failure card), the colour/glyph seam (`theme`), the ONE formatting vocabulary (`format`), execution-flow reads (`flow`), bounded output summaries (`shape`), painted source spans (`snippet`), the glyph/hint vocabulary (`vocab`) and the deterministic demo streams (`demo` — prod-shared: the `nika demo` verb replays them). One truth in, text out; no I/O lives here. |
| LOC budget | the 15k-prod workspace ratchet governs (≤1,500/file · ≤100/fn as everywhere) — admitted at ~2.6k prod src; the parent `nika-cli` drops from 14,999 (the wall that blocked two display fixes on 2026-07-10) to ~12.4k |
| File cap | ≤1,500 LOC each (max at admission: `render.rs`) |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace (`0.99.0` at admission) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L4 interface crate, same stance as `nika-cli` |
| Extraction source | `crates/nika-cli/src/display/{mod→lib,state,render,theme,format,flow,shape,snippet,vocab}.rs` + `src/demo.rs` (git-mv, history preserved). `wires.rs` STAYS in `nika-cli` (its prod edge is `verbs::graph::GraphDoc` — the graph verb's renderer, relocated to `src/wires.rs`). `nika-cli` re-exports the whole surface at its old paths (`pub use nika_display as display;` + `pub use nika_display::demo;`) — zero call-site churn. Per D-2026-07-09-N1 the descent is ONE architectural unit in TWO members — this crate spec names the parentage; the unit stays `nika-cli`'s. Precedent: `nika-dap` (2026-07-09) · `nika-cap` (2026-07-07), the same wall. |
| NIKA codes | **none** — a render surface: it formats other components' codes and never mints its own (the one-voice model stays upstream) |

---

## 1. Purpose

`nika-display` is the **run-comprehension surface**: everything between a
stream of real `nika_event::Event` values and the text a human reads.
The fold is pure (`RunView::apply` is the only mutation path), the render
is deterministic (golden tests pin exact frames via the `demo` streams),
and the crate performs **no I/O** — sinks and terminals live in the
parent `nika-cli`.

## 2. Why a crate (and why now)

`nika-cli` sat at **14,999/15,000 prod LOC** — a +1 budget, measured
2026-07-10 when two display-honesty fixes (the failed-count meter and
the failure-card code dedup · issue #393) could not land. Compacting was
insufficient, the cap is forbidden to move (it has paid five times), and
`display/` was the cleanest cut: a self-contained pure surface whose
only inverse edge was `wires.rs` (which stays, relocated). Precedent:
the `nika-dap` and `nika-cap` descents at the same wall.

## 3. Public API (the whole surface)

```text
pub mod state    { RunView · TaskRow · TaskState · str_field }
pub mod render   { frame · stream_header · stream_settled_line · stream_summary }
pub mod theme    { Theme · Role }
pub mod format   { the ONE cost/duration/size formatter vocabulary }
pub mod flow     { Interval · interval_of · lane_marks · heat_bucket }
pub mod fruit    { written_files · last_said · cautions · rehearsal — the run's fruit + form-sanity reads }
pub mod shape    { bounded type-aware output summaries }
pub mod snippet  { paint_span — rustc-grade span frames }
pub mod vocab    { hint · arrow · at_least — the glyph/hint vocabulary }
pub mod demo     { deterministic §3.3 storyboard streams (success · failure · …) }
```

## 4. Invariants

- **No I/O** — the crate never opens files, sockets or terminals.
- **Pure fold** — `RunView` is a function of the event stream; replay =
  re-render, never re-execute (the same law the `nika-dap` replayer holds).
- **Meter honesty** — a failing or repaired run's summary line never
  reads byte-identical to a clean one (`N failed · ` / `N recovered · `
  ride the meter).
- **One-voice rendering** — the surface formats upstream codes and never
  mints its own.

The project verdict renderer accepts primitive fields from its caller. Its
machine envelope uses JSON string escaping for paths, names and diagnostics,
including control characters, while preserving the compact field order.
It does not parse projects or choose the caller's exit code.
