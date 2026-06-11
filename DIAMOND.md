# 💎 Nika Diamond

The ground-up rewrite of Nika into a clean, AI-refactorable workspace
of 42 small crates. This document is the strategy overview. For
the authoritative architectural decision, see
[ADR-001 — Diamond orphan branch](docs/adr/adr-001-diamond-orphan-branch.md).

- **Branch** — `main` (default · production). Orphan branch, no shared
  history with `brouillon` (renamed 2026-05-06 from the previous
  side-name `nika-diamond` · renamed per ADR-001 amendment).
- **Brouillon** — reference only, read-only, where legacy Nika v0.79.3
  lives. Accessed via `git show brouillon:path/to/file.rs` when guidance
  is needed. Never copy-pasted — every crate is rewritten clean.

For the project's landing page, see [`README.md`](README.md). For
the forever-v0.x plan across v0.81 → v0.90 → v0.95 → v0.100 → v0.110+,
see [`ROADMAP.md`](ROADMAP.md). For the **10-year architectural horizon
(2026 → 2036)** with refined 42-crate target, 4-verb stress test
through 2036, 7-ADR queue (050-056) and per-crate detail, see
[`docs/architecture/BLUEPRINT_2036.md`](docs/architecture/BLUEPRINT_2036.md)
(proposal-grade · annual decennial review 2027-04+ · version lives in
its frontmatter, not here).

**Memory subsystem note** · Diamond and the memory cluster are
**orthogonal** (per `naming-memory-subsystem.md` v2.4 external rule).
Diamond = construction METHOD (modular crates · 12-gate admission ·
L0→L5). Memory cluster (the Connectome) = 10 L1 satellite crates + 1 L2 orchestrator (nika-rerank M13 added by the SOTA ratification 2026-06-11) ·
publishable standalone on crates.io · implements `nika-kernel` traits.
« unified Rust runtime contract » framing supersedes earlier
4-axis-combo moat language.

## Why rewrite into a Diamond

The legacy `nika-engine` reached 138,724 LOC in a single crate. Claude
Opus 1M holds ~75k LOC in context. A hypothetical AI assistant working
on the engine sees roughly 54% of it and has to guess the rest —
which produces hallucinated refactors. Over two weeks of real work,
this yielded only 156 lines of genuine change.

The Diamond solution: split the codebase into small crates, each
**≤ 15,000 LOC**, each fitting entirely in the AI context window with
its tests and kernel traits visible. Context headroom flips from ~25%
to ~93%. Hallucinated refactors stop.

The full argument is in
[`docs/architecture/ai-velocity.md`](docs/architecture/ai-velocity.md).

## What Nika is

Nika is a workflow engine for AI — a single Rust binary that reads a
YAML file and executes a DAG of verbs:

```yaml
# workflow.nika.yaml
nika: v1
workflow: summarize-article
tasks:
  - id: fetch
    invoke:
      tool: "nika:fetch"        # fetch is a builtin tool, not a verb
      with: { url: "https://example.com/article", extract: article }
  - id: summarize
    with: { text: $fetch }
    infer: "Summarize in 3 bullets: {{with.text}}"
```

Four verbs: `infer`, `exec`, `invoke`, `agent`. (Fetching a URL is
*calling a tool*, not a distinct execution model — it is the
`nika:fetch` builtin, reached via `invoke`.) A typed
schema, a taint-tracking template engine, and a layered kernel of
side effects. LLM providers speak wire-direct through the kernel http
seam (`nika-providers` · rig NOT carried per D-2026-05-22-N17); MCP
servers via `rmcp`.

## Crate architecture

42 crates at v0.90 target, expanding to ~75 at v0.100, hard cap
100 ever. Strict downward-only layering (L0 → L5):

```
L5   nika                         binary, <500 LOC composition root
L4   cli · daemon · serve · mcp-server · lsp · sdk · init · catalog-verify
L3   runtime · shield · wasm-host · sandbox-{linux,macos,windows}
L2   verb-{exec,invoke,infer,agent} · connectome (the Connectome
     orchestrator) · policy · builtin · builtin-{github,cloud,workspace} ·
     mcp · display · media-{cas,image,pdf,document,provenance} · pck
L1.5 providers (14/14 wire-direct) · infer-local (candle · ADR-091)
L1   clock · fs · http · blob · exec-runner · screen · ocr · a11y ·
     input · browser · bm25 + the Connectome satellites (hnsw · rrf ·
     rerank · fsrs · rdfs-reasoner · temporal · graph-algos ·
     autodesc-{minimal,full}) · git · keys-* · pck-{registry,store}
L0.5 kernel (facade) · kernel-{core,ai,runtime,plugin} · kernel-mock
L0   types · error · catalog · catalog-codegen · schema · event · pack ·
     binding · pck-manifest
```

(The enforced canon is
[`docs/architecture/crate-layer-registry.md`](docs/architecture/crate-layer-registry.md)
— this tree mirrors it; live admission counts come from
`scripts/refresh-status.sh`.)

Invariants enforced by CI:
- Every crate ≤ 15,000 LOC.
- Every file ≤ 1,500 LOC.
- Every function ≤ 100 lines.
- Zero `.unwrap()` / `.expect(` in `src/` (CI-enforced).
- L0 has no async, no I/O, no heavy deps (tokio, reqwest, rayon rejected).
- Every side effect behind a kernel trait (testable with `nika-kernel-mock`).

Full layer map + allowed I/O axes:
[`docs/architecture/crate-layer-registry.md`](docs/architecture/crate-layer-registry.md).

## 12 gates per crate admission

A crate enters `Cargo.toml` `members = [...]` only when **all 12**
gates are green in the same atomic commit. Full spec:
[ADR-003](docs/adr/adr-003-12-gate-admission.md).

1. **Spec** — `docs/crate-specs/<name>.md` exists (purpose, layer, LOC budget, public API)
2. **TDD** — tests written before implementation (RED → GREEN)
3. **Impl** — compiles, tests pass, no temporary code
4. **Clippy 0** — `cargo clippy --workspace --all-targets -- -D warnings`
5. **Mutation ≥ 90%** — `cargo mutants -p <name>`
6. **Property** — `proptest` on sensitive surfaces (parsers, encoding)
7. **Benchmarks** — `benches/` on hot paths (exempt otherwise, documented)
8. **Docs** — `cargo doc --no-deps` zero warnings
9. **Canary E2E** — `tests/canary-<name>.nika.yaml` (exempt L0-L1)
10. **Legacy parity** — golden test vs `git show brouillon:...` output
11. **Review swarm** — three agents in parallel, P0/P1 fixed same session
12. **Atomic commit** — one admission, one commit

## 7 pre-launch shadow zones

Before `git tag v0.90.0`, all seven must be green:

1. `nika serve` input trust (P0 prompt injection boundary)
2. Cross-provider structured output parity (~35 tests)
3. `binding/template` hardening (legacy 7,243 LOC file + 15 unwraps — auto-resolved by Phase 1 `nika-binding` rewrite)
4. L1 taint analysis at runtime (lint-only today)
5. `for_each` per-element spotlight
6. `NikaError` Display parity — auto-resolved by `nika-error` admission
7. Provider parity matrix (~72 tests)

## Forever v0.x

No v1.0 target. Each release is diamond-grade for its declared scope.
SQLite stayed on 3.x for 20 years while adding WAL, FTS, JSON1,
window functions — each release complete at that release. That is
the model. See [ADR-002](docs/adr/adr-002-forever-v0x.md).

Phase roadmap (no dates):

- **v0.81** — forward-compat seams (shipped), hygiene 10 → 31 vectors (done), `tools/` → `crates/` rename (done).
- **v0.90** — Diamond foundation: 42 crates admitted, 7 shadow zones green, pck MVP, 7 native API adapters.
- **v0.95** — the Connectome (1 L2 orchestrator + 10 L1 satellites), agent-v2 (parallel tools, ReWOO, reflection, resume), pck full with sigstore signing.
- **v0.100** — WASM plugins (wasmtime + extism sandbox), full observability (OpenTelemetry), full LSP, keys subsystem.
- **v0.110+** — Ecosystem growth; hosted Nika Cloud and Nika Enterprise are demand-driven, deferred until warranted.

Full breakdown: [`ROADMAP.md`](ROADMAP.md).

## Method

This is **not an extraction**. This is **craft**.

Each crate is rewritten from scratch, guided by the legacy code on
`brouillon` via `git show brouillon:path` but never copy-pasted. Every
`.unwrap()` becomes `?` propagation. Every file >1,500 LOC gets
split into modules. Every function >100 lines gets decomposed. Every
public API gets `#[non_exhaustive]`, a `new()` constructor, a spec,
and tests before implementation.

The butterfly on the logo is not what Nika is today. It is what
Nika becomes at v0.90 emergence, when the chrysalis opens.

## License

AGPL-3.0-or-later — see [`LICENSE`](LICENSE). Commercial relicense
(Grafana model) available for enterprise consumers. Contact:
`contact@supernovae.studio`.

© 2024-2026 [SuperNovae Studio](https://supernovae.studio)

🦋 Nika — the butterfly on the SuperNovae flag.
