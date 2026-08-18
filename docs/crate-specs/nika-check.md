# nika-check — the static judgment crate

**Layer** L0 (pure · sync · zero I/O · zero async) · **split** from
`nika-schema` 2026-07-21 at the 15k crate-size wall (the
nika-graph/nika-dap precedents) · **publish** planned with the workspace
train.

## What it is

The static judgment over a parsed workflow, in ONE crate an embedder can
depend on without pulling the CLI:

- **`analyzer/`** — the Core conformance pass (`analyze(&RawWorkflow) ->
  Result<AnalyzedWorkflow, Vec<SchemaError>>`, collect-never-fail-fast)
  and the ONE edge computation every surface projects (`analyzer::edges`
  · `G_p` = `E_d` ∪ `E_c` + the recovery reads + the topological waves).
- **the `check` ladder** (`check(&RawWorkflow) -> CheckReport`,
  `check_composed` with an injected reader) — the audit-before-it-runs:
  cost ceiling · the IFC secret-leak/egress passes · capability-escape
  fit against the declared `permits:` · hard `policy:` · the NEP-0002
  trifecta · gate reachability · schema typing/lint · unknown/missing
  tool args · hints · the composition lane · the `RunCertificate`
  (ADR-092) · `infer_permits` synthesis. Because the language is
  statically analyzable BY CONSTRUCTION (acyclic DAG · bounded
  `for_each` · non-Turing CEL · declared effects), this answers « what
  will this workflow do, cost, and touch? » with **zero API calls and
  zero tokens spent** (spec `07-conformance.md` §`nika check`).

`nika-schema` keeps its blueprint shape (THE PARSER: AST + raw + error +
keysets). The dependency points DOWNWARD-ONLY: `nika-check → nika-schema`,
never the reverse — the one shared constant (`ERROR_DOCS_BASE`) lives
beside `SpecCode` in `nika_schema::error` and is re-exported here.

## Why L0, not L1

The registry's mechanical sort (< 10 s): not the binary, not a
transport, not runtime policy, not a verb orchestrating L1 effects, not
an I/O primitive → **L0**. The code is the same pure/sync/zero-I/O code
that sat inside `nika-schema` at L0 (vector 22 no-async-in-L0 + vector
33 layer-deps bans judge it unchanged). And THREE L0 consumers make any
higher layer a `check-layering.sh` upward-dep violation: **nika-lints**
(`native_first::classify` + `analyzer::edges`), **nika-graph**
(`CheckReport` + `task_permits` + `analyzer::edges`), and
`nika-schema`'s own `skill.rs` (the docs-URL constant). L0→L0 same-layer
edges are the allowed case.

## Why a crate at all (collapse-vs-publish)

Seven consumers across L0→L4 (cli · lsp · mcp · dap · runtime ·
verb-agent · lints · graph · onboard) rode the module boundary; the
embedder/SDK consumer (VISION_2040 §1 lists `sdk` in L4) wants static
judgment without the operator surface. No existing crate is the home:
`nika-cap` is the permits *vocabulary* and never touches an AST (its own
invariant — the AST-side capability judgment is exactly what lives
here); `nika-graph` is one projection, not the judgment. Nothing in the
cluster went elsewhere — the clean cut.

## Cross-crate `#[non_exhaustive]` discipline

The raw enums (`RawAction` · `ForEachValue` · `RawCommand` ·
`VisionInput`) are `#[non_exhaustive]` in `nika-schema`; inside the
defining crate the matches compiled exhaustive, outside they demand a
wildcard. Every match in this crate carries the fail-loud arm (the
nika-graph idiom · `#[allow(clippy::unreachable, reason = …)] other =>
unreachable!`) — a future variant must teach its checker explicitly;
silently-wrong judgment is the one unacceptable outcome.

## Metrics (projected — `scripts/crate-metrics.sh nika-check`)

~22,869 LOC src (live anchor · total incl. tests — **12,455 prod** per
the ratchet's `scripts/ci/check-crate-size.sh` scope, measured
2026-08-18 after the ADR-115 descent: the analysis substrate left for
`nika-check-analyzer`, taking 14,995 → 12,455 and the headroom from
**5** lines to **2,545**) · largest file ~1,445
LOC · ~485 unit + ~39 integration tests (the conformance harness at
`tests/common` verdicts the Core + Deep tiers against the nika-spec
checkout — `NIKA_SPEC_DIR`). Benches: `parser_bench` (parse+check) ·
`refonte_baseline` (parse/analyze/check across topologies · W0 gate 8).

## The split gates (moved-code admission, per the nika-dap precedent)

| Gate | Verdict |
|---|---|
| SPEC (this document) | ✅ |
| Workspace member + layer metadata (`layers.nika-check = "L0"`) | ✅ |
| Layer-registry row | ✅ |
| crate-spec freshness vector 6 | ✅ (live anchor above) |
| public-api baseline + coverage floor row | ✅ |
| cargo-machete clean | ✅ |
| clippy `-D warnings` · fmt · cargo doc 0 warnings | ✅ |
| `cargo test --workspace --lib` + `--tests` green (conformance at the spec pin) | ✅ |
| MUTATION | inherited — the code moved mechanically from `nika-schema`, whose Gate-5 budget was measured at admission (290≤300); a fresh cargo-mutants run is the admission-tier follow-up, same class as the nika-graph/nika-dap moves |
