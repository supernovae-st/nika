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


## Paged item projection

`RunView` accumulates `task_items` by task and observation. A task start clears
its pending pages; a terminal exposes the reconstructed table only if page
order, global row indexes and all terminal counters agree. Missing, duplicate,
reordered or malformed pages yield no complete table. This fold is shared by
live rendering and trace outputs/replay; it never rewrites physical frames or
substitutes for the independent chain verifier.

The fold knows five row statuses: `ok`, `recovered`, `failed`,
`never_started` and `cancelled` (B8 · 2026-09-28 · spec 17, next MINOR after
0.121). Any other status leaves the table incomplete. `items_cancelled` must
equal the cancelled rows. Its absence, as in a terminal written before the
word existed, is accepted only when no collected row is `cancelled`; a
present non-integer count is a mismatch. Inline tables are not folded here:
an unfamiliar inline status reaches readers as uninterpreted data, which spec
17 permits, and is never coerced into a known outcome.


## 1. Purpose

`RunView::harness_media` projects image observations as a bounded sample with
the observed total, terminal count and completeness. Missing or malformed
evidence stays incomplete; a new attempt clears the previous sample. This is
a pure fold of trace events: it never opens reported paths or reads stored
images. The [TUI](nika-tui.md) displays these facts in task details.

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
pub mod model_scope { notice — render admitted envelope-model hints for a human model override }
pub mod shape    { bounded type-aware output summaries }
pub mod snippet  { paint_span — rustc-grade span frames }
pub mod vocab    { hint · arrow · at_least — the glyph/hint vocabulary }
pub mod demo     { deterministic §3.3 storyboard streams (success · failure · …) }
pub mod check_render::review { finding_rows · effect_rows · plan_lines · plan_lines_in_order · task_face · external_effects }
pub mod front_door { welcome and choice views · doctor human/JSON report cells }
pub mod repair_render { Repair · StopNotes · Refusal · render_refusals · render_stops · summary }
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

`check_render::review` (C10) holds the rows a review shows of one `nika check` report,
descended from `nika-session`'s change preview with their strings, order and limits
unchanged: the first findings (`code · message`, at most eight) and hints (`kind ·
advice`, at most four), one row per effect class the report's own permits and requirements
name, and the spend a run can reach from the report's cost envelope (no model call spends
nothing on inference, a model with no catalog price is unknown and never free, a missing
token or iteration bound stays unbounded). Pure text over the report: the caller keeps the
verdict, the path and every authority.

## Passive operator views

`front_door` also owns welcome/choice data views and their renderers, including
`front_door::doctor` and its serializable `Finding`/`Level`. The host supplies
resolved context, selected next actions, redacted facts and prepared doctor rows;
these views use the existing `Theme` without collecting, choosing or persisting.
`repair_render` owns repair report values and text, while the host applies and
judges repairs. `check_render::review` owns the candidate plan lines and task faces
formerly rendered by Session; parsing for presentation grants no authority.

`external_effects(candidate, boundary)` renders the network hosts and programs
declared by strictly parsed workflow bytes together with the check report's
inferred requirements. It retains declared loopback hosts and names unresolved
network or program requirements when the check is partial. This is the same
pure renderer used by Session previews; it opens nothing and grants no permit.

## Recorded terminal outputs

`RunView::workflow_outputs()` returns the terminal frame's recorded outputs:
`None` before a terminal, then `Outputs` with distinct absent, kept, oversized,
withheld or unreadable states. A new start clears the earlier map. A failed
run remains failed even when its terminal carries resolved values. The fold
never reconstructs outputs from a current workflow or from task homonyms.

## Data-location presentation

`front_door::DataLocus` is a passive projection of the host's resolved data
location and its explanatory line. It performs no lookup, persistence or
admission. Session retains the provider/census resolution and re-exports the
same `DataLocus` name, so existing presentation consumers retain their path.
Subscription completion transport vocabulary lives separately in
`nika-types::access::HarnessTransport`; neither projection grants Run access.

`model_scope::decision_status` projects the host-observed decision-service name,
selection origin, endpoint/deadline or refusal. It performs no selection, transport,
admission or pricing; the host supplies every fact.

The pure `activity` presentation owns `Phase`, `Activity` and the compatible text/typed sinks.
They carry the producer's phase unchanged; no runtime, model call, execution state or authority
is owned here. `nika_onboard::activity` and `nika_session::activity` preserve their old paths.

`activity::CallMark` carries producer-reported call identity and lifecycle; the model is requested,
not observed as served. Scoped presentation callbacks contain neither transport nor authority.
`activity::call_activity` derives a compiler call's phase from its typed role alone: `repair`,
`sketch-repair`, `fill-repair`, `native-repair` and `transform-repair` are Repairing, every role
that starts with `judge` (the whole request, a clause, a part asked alone, the pointer that asks
which task fails a part judged missing, an extra operation, one part over a trial run, a whole
trial run) is Checking, and any other role is Authoring. The activity card names each role in
plain words: `judge_request` and `judge_clause` « review your request », `judge_part` « review a
part of your request », `judge_point` « find the step a missing part points to »,
`judge_observed_part` « check one part against the trial run », `judge_observed` « review a
trial run » and `judge_extra` « review the workflow »; a role it does not know is shown as it
came, and the exact role stays in the event and the receipts.
The front-door's Session help text is re-exported by Session without changing its public path.

### Passive continuation and lifecycle words

`front_door::round` formats the validated round facts supplied by its owner; `RoundWords` carries summary, asked and blocked text and grants no continuation authority. `front_door::recovery` formats recovery cards and the kept-unjudged notice. `front_door::status` formats an already selected proposal, gate or Run exit. None reads files, chooses a model, verifies a candidate, starts a call or changes Session state.

`front_door::recovery::cannot_express` renders the host's already classified unfinished
preparation and human-readable reasons. Session retains classification and all authoring,
recovery, validation and authority decisions; Display performs no reading or dispatch.
