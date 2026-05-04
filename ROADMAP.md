# Nika Roadmap

> Last updated: 2026-04-29
> Current version: **v0.79.x** | Schema: `nika/workflow@0.12`
> Nika stays 0.x.x forever — see [design principles](#design-principles).

## Current: v0.65 -- Native JQ + Dashboard (DONE)

- [x] `nika:jq` builtin -- full jq expression evaluation via jaq-core
- [x] `nika:tree_data` builtin -- nested group_by for treemap/dashboard hierarchies
- [x] `eval_jq()` public API for programmatic jq from Rust
- [x] `nika:inject` -- template marker replacement
- [x] `nika:enrich`, `nika:map` with transform parameter
- [x] `extract: metadata_links` -- combined metadata + links in one fetch

## Recent: v0.63 -- Skills, Transforms, Crawling (DONE)

- [x] Auto-inject workflow `skills:` into all `infer:` task system prompts
- [x] `overwrite: true` for `nika:write`
- [x] Missing skill files now fail loud (NIKA-270)
- [x] 15 new transforms: `pluck`, `where`, `pick`, `omit`, `sort_by`, `merge`, `regex`, `base64_encode/decode`, `starts_with`, `ends_with`, `contains`, `content_hash`, `unique_urls`
- [x] `nika check` validates skill/context file paths and provider names

## Recent: v0.60-v0.62 -- Artifact API + Hardening (DONE)

- [x] Artifact API (workflow-level + task-level + binary + manifest)
- [x] Typed SSE events for `nika serve`
- [x] Checkpoints and partial results
- [x] 19-commit security hardening (2 CRITICAL + 4 HIGH)
- [x] `nika.toml` project config, `nika init` wizard
- [x] Job isolation (`NIKA_JOB_ID`, `NIKA_JOB_DIR`)

## v0.55-v0.59 -- Production Foundation (DONE)

- [x] NikaVault (XChaCha20Poly1305 + Argon2i) -- replaces OS keychain
- [x] `nika serve` V1 + V2 (subprocess -> embedded Runner, SSE streaming)
- [x] vLLM / OpenAI-compatible custom endpoints
- [x] 10-agent security audit (webhook, vault, serve, provider, daemon)
- [x] 9,000+ tests

---

## In development — foundation phase

Nika is in foundation phase. The engine is being refactored under the
**Constellation / Diamond** architecture: layered crates (L0 kernel →
L5 binary), strict trait boundaries for every side-effect (HTTP,
shell, FS, LLM, MCP), and a 12-gate per-crate admission discipline.
Quality and stability take priority over a fixed shipping date —
Nika ships when the foundation is right, not when a calendar says so.

### Documentation & website

- [ ] Mintlify docs site (supernovae-docs)
- [ ] Quickstart guide (5-minute onboarding)
- [ ] Verb reference (infer, exec, fetch, invoke, agent)
- [ ] Cookbook: 20+ real-world workflow examples
- [ ] Video walkthroughs

### Stability & polish

- [ ] Final pass on error messages (every NIKA-XXX code has a clear fix suggestion)
- [ ] `nika doctor --fix` covers all common setup issues
- [ ] Cross-platform testing (Linux, macOS, Windows)
- [ ] Performance benchmarks published

### Distribution

- [ ] Homebrew (`brew install supernovae/tap/nika`)
- [ ] crates.io (`cargo install nika`)
- [ ] GitHub Releases (pre-built binaries for Linux/macOS/Windows)
- [ ] Docker image (`docker pull supernovae/nika`)
- [ ] VS Code extension (LSP-based)
- [ ] npm bridge (`nika-napi` — TypeScript/JavaScript embedding)

### Legal

- [ ] e-Soleau INPI deposit
- [ ] Trademark review
- [ ] Privacy Policy + Mentions Legales (LCEN)
- [ ] AGPL source code link in UI

---

## Long-term direction

### NikaVault Universal Identity

Evolve NikaVault from API key storage into a universal credential
vault. OAuth2 PKCE flows, auto-refresh, import from
Doppler/1Password/Bitwarden, audit logging.

### Memory subsystem (cognitive memory layer)

A persistent cognitive memory layer for Nika workflows
(historically referenced as "Egghead" / "Cortex" — internal
codenames, retired). Embedded RDF-star graph with cognitive
mechanisms — Hebbian reinforcement, FSRS / ACT-R decay, AGM
belief revision, consolidation, dopamine gating, ontology
evolution. Built on Oxigraph 0.5.6 (W3C RDF / SPARQL / RDF-star,
no fork) with 8 L1 satellite crates — `nika-hnsw`, `nika-bm25`,
`nika-rrf`, `nika-fsrs`, `nika-graph-algos`, `nika-rdfs-reasoner`,
`nika-temporal`, `nika-autodesc` — wrapped by the `nika-memory`
orchestrator (NikaStore actor + 9 builtins + 12 cognitive
mechanisms). HNSW ephemeral cache rebuilt at boot — single file,
single binary, single transaction atomicity.

12 cognitive mechanisms · 4 memory types (working / episodic /
semantic / procedural).

**Status (2026-04-29):** Phase 0 prep ~90 % complete (21 design
docs + 4 ADRs locked). Phase 1 implementation runs 2026-05-06 →
2026-06-25 and reaches `v0.1.0` on the 8 satellite crates +
`nika-memory` orchestrator. The 8 satellites are publishable
standalone on crates.io as Rust RDF/ML ecosystem contributions;
they publish individually as ready, no monolithic release event.
Phase 2 workflow integration 2026-07-01 → 2026-08-30 (`v0.2.0`).

### Infrastructure scaling

- Stage 1: Single VPS + H100 (~100 workflows/day) — current
- Stage 2: Bigger VPS, still SQLite (~10K workflows/day)
- Stage 3: 2× App + LB + PostgreSQL (~100K workflows/day)
- Stage 4: Multi-region (~1M workflows/day)

---

## Design Principles

These apply to all roadmap items:

1. **5 verbs, forever.** No new verbs. New capabilities go through `invoke:` and builtin tools.
2. **0.x.x forever.** Nika will never be v1.0.0.
3. **Schema stability.** `nika/workflow@0.12` is the current and only supported schema.
4. **Provider-agnostic.** Same workflow, any LLM provider. No lock-in.
5. **Inference as Code.** YAML workflows are the interface. No SDK required to use Nika.
