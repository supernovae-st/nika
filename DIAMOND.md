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
the real-semver plan toward a 1.0 launch (amended D-2026-06-20-N1 · was
"forever-v0.x") — the CHANGELOG top names the latest tagged release, `main` on the next
dev version → 1.0.0 → 1.x adds the remaining crates → 2.0 the Connectome era — see [`ROADMAP.md`](ROADMAP.md). For the
**10-year architectural horizon
(2026 → 2036)** with the ADR-037 count horizon (50-90 · cap 100 ·
projected, never a gate · ruled D-2026-07-21-N1), 4-verb stress test
through 2036, 7-ADR queue (050-056) and per-crate detail, see
[`docs/architecture/BLUEPRINT_2036.md`](docs/architecture/BLUEPRINT_2036.md)
(proposal-grade · annual decennial review 2027-04+ · version lives in
its frontmatter, not here).

**Memory subsystem note** · Diamond and the memory cluster are
**orthogonal** (per `naming-memory-subsystem.md` v2.4 external rule).
Diamond = construction METHOD (modular crates · 12-gate admission ·
L0→L5). The Connectome = 1 L2 orchestrator + 10 L1 satellite crates ·
publishable standalone on crates.io · implements `nika-kernel` traits.
Moat framing: « unified Rust runtime contract ».

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
nika: summarize-article
permits:
  net: { http: ["example.com"] }
  tools: ["nika:fetch"]
tasks:
  fetch:
    invoke:
      tool: "nika:fetch"        # fetch is a builtin tool, not a verb
      args: { url: "https://example.com/article", mode: article }
  summarize:
    with: { text: "${{ tasks.fetch.output }}" }
    infer: { prompt: "Summarize in 3 bullets: ${{ with.text }}" }
```

Four verbs: `infer`, `exec`, `invoke`, `agent`. (Fetching a URL is
*calling a tool*, not a distinct execution model — it is the
`nika:fetch` builtin, reached via `invoke`.) A typed
schema, a taint-tracking template engine, and a layered kernel of
side effects. LLM providers speak wire-direct through the kernel http
seam (`nika-providers` · rig NOT carried per D-2026-05-22-N17); MCP
servers via `rmcp`.

## Crate architecture

Crate count: ADR-037 horizon **50-90** (reached additively across the 1.x
minors), hard cap 100 ever — the count is projected
(`scripts/crate-metrics.sh`), never a gate (ruled D-2026-07-21-N1).
Strict downward-only layering (L0 → L5):

```
L5   nika                         binary, <500 LOC composition root
L4   cli · daemon · serve · mcp-server · lsp · sdk · init · catalog-verify
L3   runtime · shield · wasm-host · sandbox-{linux,macos,windows}
L2   verb-{exec,invoke,infer,agent} · connectome (the Connectome
     orchestrator) · policy · builtin · builtin-{github,cloud,workspace} ·
     mcp · display · media-{cas,image,pdf,document,provenance} · pck
L1.5 providers (16/16 per canon.yaml) · infer-local (candle · ADR-091)
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

Before `git tag v1.0.0` (the first public launch · amended D-2026-06-20-N1), all seven must be green:

1. `nika serve` input trust (P0 prompt injection boundary)
2. Cross-provider structured output parity (~35 tests)
3. `binding/template` hardening (legacy 7,243 LOC file + 15 unwraps — auto-resolved by Phase 1 `nika-binding` rewrite)
4. L1 taint analysis at runtime (lint-only today)
5. `for_each` per-element spotlight
6. `NikaError` Display parity — auto-resolved by `nika-error` admission
7. Provider parity matrix (~72 tests)

## Real semver toward 1.0 (amended D-2026-06-20-N1)

Real semver toward a **1.0** public launch (was "forever-v0.x" · retired
D-2026-06-20-N1). Each release is diamond-grade for its declared scope —
the craft standard, not the scope list. SQLite shipped a 1.0 and kept
adding WAL, FTS, JSON1, window functions while staying diamond-grade at
every release — that is the model. The nine-key **LANGUAGE** envelope is
frozen forever and is orthogonal to the engine's binary version. See
[ADR-002](docs/adr/adr-002-forever-v0x.md).

Version ladder (no per-tag dates · quality > speed):

- **release-candidate grade reached at 0.91.0**: usable vertical slice (4 verbs, 16 providers per canon.yaml, effects, static-check, MCP/LSP, CLI), headless workspace build — the CHANGELOG top names the current release.
- **1.0.0-rc.N** — design-partner hardening, 7 shadow zones green.
- **1.0.0** — **first public launch**: language + installable binary, validated.
- **1.x minors** — add the remaining crates additively under the ADR-037 count horizon (50-90 · cap 100 · projected, never a gate) (pck, native API adapters, WASM plugins, full observability, full LSP, keys subsystem).
- **2.0** — **the Connectome era**: memory + cognition (1 L2 orchestrator + 10 L1 satellites, agent-v2). The next epoch.

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
Nika becomes at the 1.0 launch (amended D-2026-06-20-N1), when the
chrysalis opens.

## License

AGPL-3.0-or-later — see [`LICENSE`](LICENSE). Commercial relicense
(Grafana model) available for enterprise consumers. Contact:
`contact@supernovae.studio`.

© 2024-2026 [SuperNovae Studio](https://supernovae.studio)

🦋 Nika — the butterfly on the SuperNovae flag.
