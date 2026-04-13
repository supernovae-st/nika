# 💎 Nika Diamond

The architectural refactor of Nika into a clean, AI-refactorable diamond.

**Branch** : `nika-diamond` (this one). Orphan branch, fresh tree.
**Main** : reference only, read-only, where legacy Nika lives.

---

## What is Nika ?

Nika = **"Inference as Code"** — a single Rust binary that reads a YAML file
and executes AI workflows. No Python, no Docker, no vendor lock-in. AGPL + CLA.

```yaml
# workflow.nika.yaml
schema: "nika/workflow@0.12"
tasks:
  - id: fetch
    fetch: { url: "https://example.com/article", extract: article }
  - id: summarize
    with: { text: $fetch }
    infer: "Summarize in 3 bullets: {{with.text}}"
```

Run :
```bash
brew install nika
nika run workflow.nika.yaml
```

5 verbs : `infer` · `exec` · `fetch` · `invoke` · `agent`
63 builtin tools · 9 LLM providers · 11 MCP servers · media pipeline ·
structured output 4-layer · Shield 5-layer · vault secrets · LSP for VS Code.

---

## Why rewrite into a diamond ?

The monolithic `nika-engine` reached 138,654 LOC. Claude Opus 1M can hold
~75k LOC in context. When I (an AI) work on the engine, I see ~54% of it
and guess the rest → **I hallucinate**. Over 2 weeks of work, we produced
only 156 lines of real engine change because I kept inventing broken code.

The diamond solution : split everything into ~32-34 small crates, each
**≤15k LOC**. Every crate fits entirely in my context window with its
tests and kernel traits. I stop hallucinating. You get a reliable partner.

Full rationale : `~/.claude/projects/.../memory/project_ai_velocity_north_star.md`.

---

## Current state (2026-04-13)

- Phase 0 ✓ DONE : scaffold (workspace + CI + `.claude/` rules)
- Phase 1 ⏳ IN PROGRESS : split `nika-core` into 5 sub-crates
- Phases 2-6 : see `docs/ROADMAP.md`
- Timeline : **11-12 months** honest (not 14 weeks, not 9 months)
- Method : orphan branch, REWRITE propre (not copy-paste from main),
  every crate passes 12 gates before admission

---

## Architecture (final target)

```
                        ┌─────────────┐
   L5  BINARY           │   nika      │   composition root, <500 LOC
                        └──────┬──────┘
                               │
   L4  INTERFACES              │
     ┌────────┬────────┬───────┴──────┬────────┬─────────┐
     │        │        │              │        │         │
  nika-cli nika-lsp nika-serve    nika-sdk nika-init  nika-lints
                                                      (custom dylint)
                               │
   L3  ORCHESTRATION           │
     ┌──────────────┬──────────┴──────┐
     │              │                 │
  nika-runtime  nika-daemon      (nika-cache = module)
  (includes     (includes
   policy+      storage
   cache        module)
   modules)
                               │
   L2  DOMAIN                  │
     ┌─ Verb crates (5) ──────────────┬─ Service crates (9) ─────────┐
     │  nika-verb-exec                 │  nika-provider-rig           │
     │  nika-verb-fetch                │  nika-provider-native        │
     │  nika-verb-invoke               │  nika-provider-mock          │
     │  nika-verb-infer                │  nika-builtin                │
     │  nika-verb-agent                │  nika-mcp                    │
     │  (+ hooks agent-v2 reserved)    │  nika-vault                  │
     │                                  │  nika-display                │
     └─ Media (5 split, heavy deps) ───┘                              │
       nika-media-cas                                                  │
       nika-media-image                                                │
       nika-media-pdf                                                  │
       nika-media-document                                             │
       nika-media-provenance
                               │
   L2.5  MEMORY (future v0.95) │  (stubs reserved Phase 0 for
     nika-memory                │   Cortex integration non-breaking)
     (+3 satellites Phase 9+)
                               │
   L1  SUPPORT + EFFECTS       │
     ┌─────────────┬───────────┴─────────┬─────────┐
     │             │                     │         │
  nika-shield  nika-event  nika-lsp-core │         │
  (5-layer    (EventLog   (pure LSP     │  Effect impls :
   defense)    Traces)     intelligence) │   nika-clock
                                         │   nika-fs
                                         │   nika-http
                                         │   nika-blob
                                         │   nika-process
                                         │   nika-policy (→ module)
                                         │   nika-extract
                                         │   nika-security
                               │
   L0.5  KERNEL                │
     ┌─────────────────┬───────┴──────────────┐
     │ nika-kernel     │ nika-kernel-mock     │
     │ (traits only)   │ (pure-memory mocks   │
     │                 │  for AI-simulable    │
     │                 │  tests)              │
     │ + hooks Phase 0 │                      │
     │ memory + tool   │                      │
     │ executor        │                      │
                               │
   L0  PURE (zero I/O, zero async)
     ┌───────────────────────┐
     │   nika-core           │  AST + catalogs + types + errors + trust
     │   (monolithe          │  Split en 5 sous-crates DURING PHASE 1 :
     │    original 45k →     │    ├─ nika-error       (NIKA-XXX + NikaError)
     │    split en 5)        │    ├─ nika-catalog     (providers/models/tools)
     │                        │    ├─ nika-schema      (AST Raw → Analyzed → Lower)
     │                        │    ├─ nika-binding     (Template + 65 transforms)
     │                        │    └─ (dag merged into schema)
     └───────────────────────┘

TOTAL : 32-34 diamond crates + 3 memory satellites (Phase 9+)
Each ≤15k LOC · file ≤1500 · fn ≤100 · 0 unwrap src/
Every I/O behind kernel trait · Tests with kernel-mock (pure memory)
```

---

## Complete crate list (alphabetical)

| # | Crate | Layer | LOC budget | Purpose | Status |
|---|-------|-------|-----------|---------|--------|
|  1 | nika                    | L5   | <500  | Binary entry point, composition root | PLANNED |
|  2 | nika-binding            | L0   | ≤15k  | Template engine + 65 transforms + resolve (split from nika-core) | PHASE 1 |
|  3 | nika-blob               | L1   | ~1k   | DiskBlobStore (blake3 CAS) | PHASE 2 |
|  4 | nika-builtin            | L2   | ~11k  | 63 builtin tools (core+file+data+introspection) | PHASE 3 |
|  5 | nika-catalog            | L0   | ~5k   | Static catalogs (providers/models/transforms/builtins) | PHASE 1 |
|  6 | nika-cli                | L4   | ~20k  | CLI subcommands (run/check/test/...) | PHASE 4 |
|  7 | nika-clock              | L1   | <1k   | SystemClock (tokio::time, ZST) | PHASE 2 |
|  8 | nika-daemon             | L3   | ~8k   | Background scheduler + IPC + cron (includes storage module) | PHASE 4 |
|  9 | nika-display            | L2   | ~13k  | CLI renderers (Renderer trait) | PHASE 3 |
| 10 | nika-error              | L0   | ~3k   | NikaError enum + NIKA-XXX codes (split from nika-core) | PHASE 1 |
| 11 | nika-event              | L1   | ~5k   | EventLog + TraceWriter + NDJSON (includes macros inlined) | PHASE 2 |
| 12 | nika-extract            | L1   | ~1k   | 9-mode fetch extraction (pure) | PHASE 2 |
| 13 | nika-fs                 | L1   | ~1k   | TokioFs (FsRead + FsWrite splinters) | PHASE 2 |
| 14 | nika-http               | L1   | ~2k   | ReqwestClient + SSRF defense | PHASE 2 |
| 15 | nika-init               | L4   | ~5k   | `nika init` project wizard | PHASE 4 |
| 16 | nika-kernel             | L0.5 | ~3k   | Kernel traits (Provider, ToolExecutor, MemoryStore, ...) | PHASE 1 |
| 17 | nika-kernel-mock        | L0.5 | ~2k   | Pure-memory mocks for all kernel traits | PHASE 1 |
| 18 | nika-lints              | L4   | ~1k   | Custom dylint lints for invariants #11/#16/#23/#24 | PHASE 4 |
| 19 | nika-lsp                | L4   | ~15k  | LSP server (includes lsp-core merged) | PHASE 4 |
| 20 | nika-mcp                | L2   | ~9k   | rmcp client pool + retry + 50MB cap | PHASE 3 |
| 21 | nika-media-cas          | L2   | ~2k   | CAS store (MediaRef, MediaBudget) — no heavy deps | PHASE 3 |
| 22 | nika-media-document     | L2   | ~3k   | svg/chart/readability/html_to_md | PHASE 3 |
| 23 | nika-media-image        | L2   | ~4k   | thumbnail/convert/strip/optimize/phash | PHASE 3 |
| 24 | nika-media-pdf          | L2   | ~1.5k | pdf_extract | PHASE 3 |
| 25 | nika-media-provenance   | L2   | ~2k   | c2pa/verify/qr | PHASE 3 |
| 26 | nika-memory             | L2.5 | ~3k   | Cortex façade stub (NullMemoryStore in Phase 0) | PHASE 1 |
| 27 | nika-process            | L1   | ~2k   | TokioShell (kill_on_drop, cancel) | PHASE 2 |
| 28 | nika-provider-mock      | L2   | ~1k   | Deterministic test provider | PHASE 3 |
| 29 | nika-provider-native    | L2   | ~3k   | mistral.rs GGUF local, feature-gated | PHASE 3 |
| 30 | nika-provider-rig       | L2   | ~4k   | 7 cloud + 7 OpenAI-compat via rig-core | PHASE 3 |
| 31 | nika-runtime            | L3   | ~12k  | Runner + dispatch + binding (includes policy/cache) | PHASE 4 |
| 32 | nika-schema             | L0   | ~15k  | AST Raw→Analyzed→Lower + DAG (split from nika-core) | PHASE 1 |
| 33 | nika-security           | L1   | ~3k   | Blocklist + injection guards + redact | PHASE 2 |
| 34 | nika-sdk                | L4   | —     | ❌ DELETED (0 consumers, speculative) | N/A |
| 35 | nika-serve              | L4   | ~4k   | HTTP API server | PHASE 4 |
| 36 | nika-shield             | L1   | ~5k   | 5-layer defense (trust/spotlight/canary/caps/validation) | PHASE 4 |
| 37 | nika-vault              | L2   | ~1.5k | XChaCha20Poly1305 + Argon2i secrets | PHASE 3 |
| 38 | nika-verb-agent         | L2   | ~12k  | Agent multi-turn loop + hooks v2 | PHASE 3 |
| 39 | nika-verb-exec          | L2   | <1k   | `exec:` verb via ShellExecutor | PHASE 3 |
| 40 | nika-verb-fetch         | L2   | ~2k   | `fetch:` verb via HttpClient | PHASE 3 |
| 41 | nika-verb-infer         | L2   | ~5k   | `infer:` verb + streaming + structured | PHASE 3 |
| 42 | nika-verb-invoke        | L2   | ~1k   | `invoke:` verb via BuiltinRouter + McpPool | PHASE 3 |

**34 crates admitted** (= 42 entries − 1 deleted (`nika-sdk`) − 7 merged-as-modules).

**Future (Phase 9+ v0.95)** : 3 memory satellites = `nika-memory-store` (Oxigraph),
`nika-memory-embed` (HNSW), `nika-memory-recall` (FSRS-6). Plus agent-v2 features
added to `nika-verb-agent` (parallel tools, ReWOO, reflection, resume, compression).

---

## 12 gates per crate admission

A crate enters `Cargo.toml` `members = [...]` only when ALL 12 gates are green :

1. **SPEC** — `docs/crate-specs/<name>.md` exists
2. **TDD** — tests written before impl (RED → GREEN)
3. **IMPL** — compiles, tests pass, no `# TEMP` without removal plan
4. **CLIPPY 0** — `cargo clippy --workspace --all-targets -- -D warnings`
5. **MUTATION ≥90%** — `cargo mutants -p <name>`
6. **PROPERTY** — proptest if sensitive (security, parsers, encoding)
7. **BENCHMARKS** — `benches/` if hot path (exempt if documented)
8. **DOCS** — `cargo doc --no-deps` 0 warnings
9. **CANARY E2E** — `tests/canary-<name>.nika.yaml` passes (exempt L0-L1)
10. **PARITY LEGACY** — golden test vs `git show main:...` output
11. **REVIEW SWARM** — 3 agents parallel, P0/P1 fixed same session
12. **ATOMIC COMMIT** — `feat(<name>): admit to workspace — all 12 gates passed`

---

## 7 pre-launch gates (shadow zones)

Before `git tag v0.90.0`, these 7 MUST be green :

1. `nika serve` input trust (P0 prompt injection hole)
2. Cross-provider structured output parity (35 tests)
3. `binding/template` rewrite (7,243 LOC legacy + 15 unwraps → resolved by Phase 1 nika-binding)
4. L1 Taint analysis runtime (lint-only today)
5. `for_each` per-element spotlight (Sprint 3 deferred)
6. NikaError Display parity tests (95% variants sans golden → resolved by Phase 1 nika-error)
7. Provider parity matrix (72 tests)

Details : `~/.claude/projects/.../memory/PRE_LAUNCH_GATES.md`.

---

## 2-year roadmap

```
v0.79 (main)  →  v0.90 (diamond, +11-12 months)  →  v0.95 (Cortex + agent-v2, +6-8 months)
             →  v1.0 (Nika Cloud hosted, +6 months)  →  v1.x (Nika Enterprise)
```

- **v0.90** (target) : 34 crates admitted, 7 pre-launch gates green, diamond complete.
- **v0.95** : nika-memory/Cortex (Oxigraph + HNSW + FSRS-6) + agent-v2 features.
- **v1.0** : Nika Cloud (hosted workflow runner, Tailscale model).
- **v1.x** : Nika Enterprise (on-prem + SSO + web UI, Grafana model).

---

## Building

```bash
# On nika-diamond branch (this one)
cd tools
cargo check --workspace        # workspace compiles
cargo nextest run --workspace  # tests (nextest for process isolation)
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check               # licenses + advisories + layer bans
cargo mutants -p <crate>       # mutation testing before admission
```

Crate admission commits use `scripts/ci/` gates (bash ratchets use clippy
as source of truth, see `PHASE_1_AUDIT.md` in repo root).

---

## Authority for all decisions

If anything is unclear, consult in this order :

1. `~/.claude/projects/.../memory/POST_AUDIT_REVISIONS.md` — supreme authority
2. `~/.claude/.../PRE_LAUNCH_GATES.md` — shadow zones
3. `~/.claude/.../HANDOFF_PHASE_1_REVISED.md` — current execution plan
4. `.claude/CLAUDE.md` — per-repo rules
5. `.claude/rules/*.md` — enforcement patterns

If two docs contradict, higher in the list wins.

---

## License

AGPL-3.0-or-later (public) + Commercial License (enterprise).
See `LICENSE` and `COMMERCIAL_LICENSE.md`.

© 2024-2026 SuperNovae Studio · [supernovae.studio](https://supernovae.studio)

🦋 Nika — the butterfly on the SuperNovae flag.
