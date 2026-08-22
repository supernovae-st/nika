---
id: ADR-092
title: "Make nika check a verifier, not a linter — the static-analysis ladder on a unified flow IR"
status: proposed
date: 2026-06-11
phase: ""
deciders: ["@ThibautMelen"]
tags: [nika-check, static-analysis, ifc, security, moat, sota-2040]
affects_crates: [nika-schema]
affects_layers: [L0]
supersedes: []
superseded_by: []
related: ["ADR-093", "ADR-094", "ADR-096", "ADR-098", "ADR-106"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: [NIKA-SEC-004, NIKA-VAR-009]
timeline: "incremental · IFC + capability inference first · the rest as a ranked roadmap"
follow_ups: []
---

# ADR-092: Make `nika check` a verifier, not a linter — the static-analysis ladder on a unified flow IR

## Context

Nika is the **only** AI workflow language that is statically analyzable BY
CONSTRUCTION — the DAG is acyclic, `for_each` is bounded, CEL is non-Turing,
and effects are declared (`permits:`). Every Turing-complete competitor
(Temporal/Airflow/LangGraph in Python; GitHub Actions' `${{ }}` + arbitrary
steps) makes the questions below **undecidable** for their workflows. This is
the moat: we can prove properties before a single token is spent; they cannot.

`nika check` shipped 2026-06-11 (then crates/nika-schema/src/check/ — the cluster
descended to `crates/nika-check/src/` 2026-07-21 · ~700 LOC ·
commits `9dfe2fda6`..`0a26e8703`) with four reports: the wave **plan**, the
**cost ceiling**, the **secret-leak** scan, and the **capability-escape** scan
against `permits:`. A 3-angle adversarial review (commit `9a0c20510`) hardened
it — but also exposed the ceiling of the current design: these are **N
independent heuristic walks** over the same AST (`cost::ceiling`,
`secrets::scan_leaks`, `permits_fit::scan_escapes`, `analyzer::analyze` each
re-walk), and they **detect** rather than **prove**. The secret-leak scan is
one-hop: it misses a `with:`-aliased secret (`with: { t: ${{ secrets.x }} }`
then `${{ with.t }}` into an exec) — a confirmed false negative.

The strategic question (operator, 2026-06-11): *what separates "a good linter"
from "10 years ahead of everyone, SOTA-2040, best algorithms, best research"?*
The answer is the **program-analysis ladder**: climb from ad-hoc scans to
**formal inference over a single annotated IR** — information-flow control,
effect/capability inference, dataflow typing, symbolic cost, SMT reachability,
termination certificates — each emitting a **machine-verifiable guarantee**.

## Decision

**Reframe `nika check` from a set of heuristic detectors into a verifier built
on a single unified flow IR, and climb the analysis ladder in moat×feasibility
order.** Concretely:

1. **A unified `FlowFacts` IR** (this ADR's keystone) — replace the N
   independent walks with ONE topological pass producing a typed,
   taint-annotated, cost-annotated fact base from which every report reads.
   Sound by a real CS argument: **because the dependency graph is acyclic by
   construction, a single topological-order pass computes the least fixpoint**
   of any monotone dataflow analysis over it.
2. **Information-Flow Control (IFC) — provable non-interference** (first moat,
   ships with this ADR). A confidentiality lattice (Denning 1976) traces
   `secrets.X` taint through `with:` aliases, task outputs, and the DAG. Fixes
   the `with:`-aliased false negative AND adds the `outputs:`-egress sink
   (a secret leaving the run as the return value). Claim: *« this workflow
   provably cannot leak its secrets to an observable sink »* — non-interference,
   which no workflow engine offers.
3. **Capability inference, not just verification** (`--infer-permits`). Infer
   each task's effect signature, compose up the DAG, and SYNTHESIZE the
   tightest `permits:` block. `nika check --infer-permits` writes your minimal
   security boundary for you (object-capability calculus over workflows).

The ranked roadmap (the rest of the ladder · sequenced, not vapor):

| # | Capability | Research basis | Moat | Status |
|---|---|---|---|---|
| 1 | **IFC taint-trace** (non-interference) | Denning 1976 · Jif · FlowCaml · LIO | ⭐⭐⭐ | ✅ shipped (`check/flow.rs`) |
| 2 | **Capability inference** (`--infer-permits`) | E-lang object-capabilities · Koka effects | ⭐⭐⭐ | ✅ shipped (`check/permits_infer.rs`) |
| 3 | **Unified FlowFacts IR** | rust-analyzer HIR · single annotated pass | ⭐⭐ (enabler) | ✅ shipped (taint slice) |
| 4 | **Dataflow schema typing** — `${{ tasks.A.output.field }}` type-checked transitively vs A's schema | bidirectional type inference | ⭐⭐⭐ | ✅ shipped (`check/schema_typing.rs`) |
| 5 | **Symbolic cost intervals** — `[min,max]` over retry/agent-turns/`when:` branches; input-token bounds | RAML (Hoffmann) · WCET | ⭐⭐ | ✅ shipped (structural slice — retry × fan-out × `when:` gates; input-token side stays out per the output-ceiling convention) |
| 6 | **Gate reachability** — dead-task + bad-status-literal over `when:` (the no-SMT slice: abstract interpretation over the 4-status terminal domain + Kleene-3 bounded enumeration — acyclicity makes it polynomial per Prinz/Schwanen/van der Aalst 2026, arxiv.org/abs/2602.02447, vs EXPSPACE general per Blondin et al. 2022, arxiv.org/abs/2201.05588; mutual-exclusion + empty-`for_each` stay roadmap, SMT never needed for the acyclic class) | abstract interpretation · workflow-net reachability | ⭐⭐ | ✅ shipped (`check/reach.rs` · dead-task slice) |
| 7 | **Termination + resource certificate** — termination is a THEOREM of the language (acyclic · `for_each` bounded · retries capped · agent turn-capped default 10); the certificate adds the quantitative envelope: degree-1 parametric bounds `c + Σ aᵢ·\|taskᵢ\|` on task-attempts / LLM calls / effect calls (AARA without the LP solver — the workflow IS its own typing derivation · Hoffmann/Das/Weng 2016 arxiv.org/abs/1611.00692 · line active in Chu/Guo/Hoffmann 2026 arxiv.org/abs/2603.02260) | AARA degree-1 · resource polynomials | ⭐⭐ | ✅ shipped (`check/certificate.rs`) |
| 8 | **Query-based incremental IR** — Salsa demand-driven; edit a task → re-analyze only the affected sub-DAG | Salsa (rust-analyzer) · Adapton | ⭐ (LSP infra) | roadmap |
| 9 | **Differential + property conformance** — ONE engine diffed against ITSELF across equivalence transformations (the single-system reformulation of differential testing · Wu/Zheng/Yang/Yu 2025 arxiv.org/abs/2504.04321 · methodology lineage Ba/Jiang/Rigger 2025 arxiv.org/abs/2508.16307): a generator emits valid workflows through the REAL front door + 3 relations — R0 generator↔engine validity · R1 task-order permutation invariance · R2 alpha-renaming invariance (engine fuzzing across N engines stays roadmap — needs a 2nd engine) | metamorphic testing · equivalence transformations | ⭐⭐ | ✅ first slice (`check/metamorphic.rs` · cfg(test)) |

Explicitly **rejected for v1**: a full Salsa rewrite (premature before the LSP
needs it), an SMT dependency (heavy; the `when:`-reachability win is real but
later), and over-tainting `infer`/`agent` outputs (the provider is operator-
chosen and trusted by that choice — a secret in a prompt is provider-bound by
design, not a leak; the model response is not a verbatim echo).

## Consequences

### Positive
- **A moat no Turing-complete competitor can cross** — non-interference,
  capability inference, and reachability are *undecidable* for Python-based
  engines. Ours is decidable because the language is analyzable by construction.
- **The `with:`-aliased false negative is fixed** — taint propagation is
  transitive, not one-hop.
- **The N-walks architecture smell is resolved** — one annotated pass, the
  `FlowFacts` IR, is the read surface for every report (the altitude fix).
- **`--infer-permits` is a "oh, THAT's clever" headline** — the file writes its
  own minimal security boundary.

### Negative
- **The IFC fixpoint is more code + more reasoning** than a substring scan; the
  taint trace must be carried for diagnostics. Accepted: the security guarantee
  is the moat.
- **Conservative tainting risks false positives** (a tainted output flowing
  widely). Mitigated: `infer`/`agent` outputs are not over-tainted (the
  trust-model carve-out), and findings carry the full chain so they are
  auditable, not opaque.
- **The roadmap (#4-#9) is multi-session** — this ADR ships #1-#3 and a real
  IR slice; the rest is documented, sequenced work, not promised vapor.

### Neutral
- The IR is internal to `nika-schema` (L0); no public-API or envelope change.
- The CLI surface (`--infer-permits`, the report) polishes into `nika-cli`
  at step 19; the engine half (runtime `NIKA-SEC-004`) is the L3 runtime's job.

## Evidence / Affected code

- `crates/nika-check/src/` -- the shipped `nika check` (mod/cost/secrets/permits_fit)
- `crates/nika-check/src/flow.rs` -- the IFC engine (this ADR · ladder #1+#3)
- `crates/nika-check/src/permits_infer.rs` -- capability inference (ladder #2 ·
  `infer_permits()` + `InferredPermits::to_yaml()` · sound-by-honesty notes ·
  round-trip property: the inferred block re-checks clean)
- `crates/nika-check/src/permits_fit.rs` -- `static_program` + `builtin_effect`
  (the shared extraction/classification surfaces · fixed the dynamic-argv[0] false
  positive both sides · read/write/edit/grep/fetch/webhook-notify covered, glob
  excluded as statically undecidable)
- `crates/nika-check/src/schema_typing.rs` -- dataflow schema typing (ladder #4 ·
  deep `tasks.X.output.<path>` references resolved against X's `schema:` JSON Schema or
  `output:` binding names · properties/items/anyOf descent · explicit
  `additionalProperties: true` honored as opaque · typo'd fields caught with zero tokens)
- `crates/nika-tmpl/src/expression/refs.rs` -- `walk_chains` shared chain-flattening
  core (`expr_refs` + `task_output_paths` consume one walker — no drift)
- `crates/nika-check-analyzer/src/dag.rs:235` -- `topo_waves`, reused for the fixpoint
  order (ex `crates/nika-check/src/analyzer/dag.rs` -- the analyzer became its own crate)
- `crates/nika-tmpl/src/expression/refs.rs` -- `expr_refs`/`NamespaceRef`, the taint extractor
- `crates/nika-types/src/suggest.rs` -- deterministic did-you-mean core (moved out of `check/` when the analyzer adopted it)
  (Damerau-Levenshtein · rustc threshold · lexicographic tie-break — the diagnostic
  model: every finding carries its machine-applicable repair)
- `crates/nika-check/src/tools.rs` -- unknown `nika:` builtin detection vs the
  closed catalog (same `all_builtins()` source as the codegen enum — no drift)
- `crates/nika-check/src/schema_lint.rs` -- authored `schema:` verification
  (the static half of structured-output reliability: required∉properties · type
  names · empty enum, with fixes)
- the examples/check/ scaffold (retired 2026-07-29 · step 19 shipped) --
  `--infer-permits` + `--json` runnable
  surface at ADR time (the agent repair loop: check → apply emitted fixes → re-check → clean,
  e2e-proven convergent); the living surface is `nika check` in
  `crates/nika-cli/src/verbs/check/`
- Commit `9a0c20510` -- the review that exposed the one-hop + N-walks ceiling
- `docs/crate-specs/nika-schema.md` §11bis -- the shipped surface + honest gaps

## Alternatives considered

### Alt A — keep adding heuristic scans
Each new property = another independent walk. Rejected: O(N) walks, no shared
fact base, the `with:`-aliased class proves heuristics miss transitive flows.

### Alt B — full Salsa/query rewrite first
Build the incremental IR before any analysis. Rejected: premature — Salsa pays
off for an LSP over 1000-task workflows, which we don't have yet; the taint
fixpoint is the value now, the incremental layer is roadmap #8.

### Alt C — pull in an external IFC/SMT library
Rejected for v1: heavyweight deps for a self-contained, sovereign L0 crate; the
2-point lattice + topological fixpoint is small, exact, and dependency-free.

## Related

- ADR-002 (real semver toward 1.0 · amended D-2026-06-20-N1) — the analyzable-by-construction invariants this builds on
- ADR-003 (12-gate admission) — `nika-schema` is admitted; this extends it
- `docs/crate-specs/nika-schema.md` §11bis — the shipped `nika check` surface
- Research: Denning « A Lattice Model of Secure Information Flow » (CACM 1976) ·
  Myers Jif · Pottier & Simonet FlowCaml · Stefan et al. LIO ·
  Hoffmann RAML · de Moura Z3 · the rust-analyzer/Salsa demand-driven model
