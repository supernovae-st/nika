# Crate spec — `nika-check-analyzer`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of the admitted `nika-check` unit · ADR-115 · D-2026-07-09-N1 · 2026-08-18) |
| Layer | L0 — pure, sync, zero I/O, zero async (the ladder and every verdict of policy stay in `nika-check`) |
| Design | the analysis substrate: Core conformance over a parsed `RawWorkflow` · the derived DAG edges · topological waves · the jq compile-check · the JSON Schema meta-check |
| IMPL | 5688 LOC src · max file 1169 · 142 unit tests (2026-08-18 live · `scripts/crate-metrics.sh nika-check-analyzer`) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal member of the `nika-check` unit |
| NIKA codes | none minted here — the analyzer speaks `SchemaError` in the spec's own voice (`NIKA-DAG-*` · `NIKA-VAR-*` · `NIKA-TYPE-*`) |

## 1. Purpose

`nika-check` grew to **14,995 prod LOC against the 15,000 cap** — five
lines from a push that blocks. This member carries the plane every other
module reads and which reads none of them back:

- `analyze` → `AnalyzedWorkflow` · the Core conformance pass, collecting
  (never fail-fast) so an author sees the whole diagnosis in one go
- `edges` · the derived DAG edges every surface projects · `dag` · the
  topological waves
- `scan` · `builtin_shape` · `types_contract` · `schema_paths`
- `jq_lint` · the jaq compile-check (the SAME jaq the runtime builtin
  compiles with — no check↔runtime drift)
- `schema_lint` · the jsonschema meta-check (`validator_for`, the SAME
  one the runtime compiles a `schema:` with)
- `model_scope` · explicit infer/agent model pins and child invocation sites
  that an envelope override does not replace; declaration facts only, with
  no model resolution, billing judgment or child loading
- `rendered` · the pre-effect model resolver (`resolve_model_expr`,
  `resolved_infer_models`): a `model:` value as known before any effect,
  from the operator's bindings, a declared default, a const or a `with:`
  alias. It descended verbatim from `nika-runtime`'s `admit.rs` (B9 ·
  2026-09-28) to sit beside `static_literal_of` and `bare_static_ref`, which
  it builds on, and to give the runtime's 15k wall room for the launch gates
  that share it. `nika-runtime` re-exports `resolve_model_expr` at its
  historical path. `rendered_models` applies the same resolver to the whole
  workflow: every envelope or task `model:` expression a binding, a default,
  a const or an alias decides becomes that literal, so the launch gates judge
  the seats the run will use with the checker's own laws; a `model:` only
  the run decides stays an expression. `rendered_collections` (B11) seats
  the invocation's bindings the same way for fan-out: a `for_each` over a
  bare `${{ inputs.<name> }}` the invocation binds iterates the bound value,
  as the run's own binding does, so the cost floor counts the operator's
  items (an array's length; anything else an unknown count, never the
  declared default's). Pure, like the rest of this plane: it resolves
  values, and it never rules on spend or authority
- `builtin_floor` · the catalog's static reads the runtime's budget gates
  price with (`priced_builtin_floor`, `unpriced_cloud_seat`): what a
  workflow's priced `invoke:` tasks must spend on their cheapest path, and
  whether a seat is a third-party cloud route the catalog does not price.
  Descended verbatim from `nika-runtime`'s `admit.rs` (B9 phase C ·
  2026-09-28) so the runtime's 15k wall has room for the dispatch guard that
  shares them; the runtime keeps every refusal and its wording
- `task_scope` · the `--task` ancestor-cone cut (`scope_to_task`): a pure
  graph walk over `edges::producer_ids` that keeps the target and its
  ancestors in document order, drops `outputs:`, ends on a cycle with each
  task once, and refuses an unknown id with the declared set. Descended
  verbatim from `nika-runtime`'s `admit.rs` (B11 · 2026-09-28) so the
  runtime's 15k wall has room for the cleanup lane's money law;
  `nika-runtime` re-exports it at its historical path, and the run verb and
  the Service keep the gate and the re-check around it

The boundary was measured, not themed · **33** edges point into this
plane from the rest of `nika-check`, and **0** point back out.

## 2. Surface law

`nika-check` re-exports this member at its historical path —
`pub use nika_check_analyzer as analyzer;`. Every call site inside and
outside keeps writing `nika_check::analyzer::…` and `crate::analyzer::…`
unchanged: the boundary moved, the surface did not. Downstream callers
never name this crate; its `public-api.txt` is the split's receipt.

## 3. Discipline

- **No verdict of policy here.** Permits fit, capability escape,
  secret-leak IFC, consent, the authored `lift:` doors, the trifecta and the
  `RunCertificate` stay in `nika-check`. This member derives the SHAPE a
  workflow has; it never rules on the authority it takes.
- The one authority-adjacent read is `jq_lint`'s
  `nika_cap::is_withheld_jq_native` — the withheld-native list (`env`,
  refused since 2026-08-15). That is a compile-check over the jq
  program, not a permits decision.
- L0 forever · pure and sync. Any I/O, async or clock here is an
  upward-dep violation (`check-layering.sh`); `jsonschema` is pinned
  `default-features = false` precisely so its retrievers compile out.
- Tests that exercise the ladder end-to-end (`crate::check` ·
  `check_composed`) belong in `nika-check` and reach this plane through
  the re-exported path.
