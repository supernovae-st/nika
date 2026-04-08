# Constellation Handoff — 2026-04-08

> **Copy-paste this entire file as context for a fresh Claude Code session.**
> **It contains everything needed to continue the Constellation architecture refactor.**

---

## WHERE WE ARE

- **Version:** v0.79.0 (Shield Sprint 2 MERGED, PR #107, 37 commits, +101 tests)
- **Branch:** main
- **Last commit:** `99cba267b` chore(license): add SPDX headers to all 862 .rs source files
- **Test count:** 10,666+ (cargo test --workspace --lib)
- **Binary:** 112 MB release
- **Crates:** 17 (target: ~44)
- **LOC:** ~555k Rust (160k in nika-engine monolith), 862 .rs source files with SPDX headers
- **Shield:** 6-layer defense fully wired. 87% coverage. SECURITY.md done.
- **Constellation:** UNBLOCKED. Plan v2.1 written. 29 research agents completed.
- **Launch readiness:** FUNDING.yml, CODE_OF_CONDUCT.md, SPDX headers, commercial license note all done
- **Launch:** May 5, 2026 (J-27 from Apr 8)

---

## MANDATORY READS (in order)

### 1. Project conventions
- `nika/CLAUDE.md` — project overview, 5 verbs, Shield section, secrets, testing
- `tools/nika/CLAUDE.md` — crate architecture, error codes (NIKA-XXX), testing rules

### 2. Architecture plans (the meat)
- `docs/plans/2026-04-08-constellation-v2-mega-plan.md` — **THE PLAN** (sections 1-17 = v2, section 18 = v2.1 deltas)
  - Section 3: Target ~32-44 crate architecture with ASCII layer diagram
  - Section 4: Dead scaffolding inventory (EventEmitter, error_domains, rstest, LSP, TUI features)
  - Section 5: 10 trait ecosystem specs
  - Section 6: nika-macros proc-macro crate (4 derives + 1 declarative)
  - Section 7: God file decomposition (5 files, 27k LOC)
  - Section 8: 20-phase roadmap with daily schedule
  - Section 17: Implementation prompts ready to paste
  - **Section 18: v2.1 revisions** (nika-kernel L0.5, 5 verb crates, linkme, trait_variant, bundles, TaskScope splinters, big-bang OK because v0)

### 3. Shield (already shipped, context for understanding trust system)
- `docs/plans/2026-04-07-nika-shield-mega-plan.md` — original Shield plan
- `docs/plans/2026-04-07-nika-shield-implementation-prompt.md` — execution prompt used for Sprint 1+2
- `SECURITY.md` — threat model document

### 4. Architecture rules
- `dx/.claude/rules/architecture.md` — current + target crate layering, updated for v0.79.0

### 5. Memory (for cross-session context)
- `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/MEMORY.md` — index
- `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_architecture_refactor_2026_04_08.md`
- `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_prompt_injection_research_2026_04_07.md`
- `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_nika_shield_review_findings.md`

---

## THE VISION — Nika Ecosystem Blueprint

### Core philosophy
- **CONNECT, DO NOT DELETE.** Dead scaffolding gets wired, not removed.
- **Big-bang cutovers OK.** v0 = zero users = zero backward compat burden.
- **One aggregate root per bounded context.** 10 bounded contexts mapped.
- **Every side effect behind a trait.** 10 traits, all mocked, all conformance-tested.
- **Every extension point is first-class.** 14 extension categories, 7 linkme slices, unified plugin crate.
- **One source of truth per config knob.** 8-layer precedence, nika-config crate.
- **One spine for all signals.** nika-telemetry with events + metrics + spans + logs + redaction.

### Target ~44 crates (from 17)

```
L0    nika-core               ONE kernel crate (38k LOC, pure, zero I/O)
L0.5  nika-kernel             10 trait defs, zero impls (~800 LOC)
      nika-kernel-mock        Hand-written mocks + conformance tests
      nika-macros             4 proc-macro derives + transform! declarative
      nika-plugin             Unified plugin system (7 linkme distributed_slices)
L1    nika-config             Unified config hierarchy (8 layers, live reload)
      nika-telemetry          Unified observability spine (events + metrics + spans + redaction)
      nika-shield             Trust, spotlight, canary, capabilities (from Sprint 2)
      nika-lsp-core           LSP intelligence (consolidated from engine/lsp/)
      nika-clock              SystemClock impl
      nika-fs                 TokioFs impl
      nika-http               ReqwestClient impl + SSRF + 9 extraction modes
      nika-exec-runner        TokioShell impl + blocklist
      nika-blob               DiskBlobStore impl (ex-CasStore)
L2    nika-provider           LLM providers (10 impls behind Provider trait)
      nika-builtin            63 builtin tools (sealed BuiltinTool + linkme)
      nika-mcp                MCP client/server
      nika-media              CAS, image ops (BlobStore trait consumer)
      nika-storage            Storage abstraction
      nika-vault              Encrypted secrets
      nika-verb-core          VerbExecutor trait
      nika-verb-util          6 shared patterns (provider chain, coerce, tokens, redact, template, schema)
      nika-verb-infer         InferVerb (2149 LOC, 12 fields)
      nika-verb-exec          ExecVerb (433 LOC, 6 fields)
      nika-verb-fetch         FetchVerb (2720 LOC, 9 fields)
      nika-verb-invoke        InvokeVerb (519 LOC, 4 fields)
      nika-verb-agent         AgentVerb (1900 LOC, 14 fields)
L3    nika-runtime            Runner, scheduler, verb dispatch (~30k LOC)
      nika-daemon             Background daemon, cron
      nika-cache              LLM + fetch cache (trust-aware keys)
L4    nika-cli                CLI subcommands + verbs/ (~6k after main.rs split)
      nika-tui-widgets        Reusable ratatui components (~10k)
      nika-tui-core           TuiState, events (~15k)
      nika-tui-views          Studio + Command + Control (~14k)
      nika-tui-app            Main event loop (~3k)
      nika-tui                Facade re-export
      nika-lsp                LSP binary
      nika-serve              HTTP API server
      nika-sdk                Embedded SDK
      nika-init               Project scaffolding
      nika-display            Formatters, renderers
L5    nika                    Binary composition root (<500 LOC)
L_dev xtask                   62 architectural invariants enforcement
      nika-conformance        Trait conformance tests (~110 test cases)
      nika-test-fixtures      Shared workflows, mocks, responses
```

### 10 Bounded Contexts (DDD)

1. **Authoring** (YAML to AST) -> nika-core
2. **Execution** (run the DAG) -> nika-runtime
3. **Inference** (talk to LLMs) -> nika-provider
4. **Effects** (IO boundary) -> nika-clock/fs/http/exec/blob
5. **Security** (Shield) -> nika-shield
6. **Observability** -> nika-telemetry
7. **Storage** -> nika-vault, nika-cache, nika-storage
8. **Presentation** -> nika-cli, nika-tui-*, nika-lsp, nika-serve
9. **Extension** -> nika-builtin, nika-plugin
10. **Scheduling** -> nika-daemon

### 10 Traits (all in nika-kernel)

1. Clock (trait_variant::make)
2. Filesystem (trait_variant::make)
3. HttpClient (trait_variant::make)
4. ShellExecutor (trait_variant::make)
5. BlobStore (trait_variant::make)
6. Provider (async_trait - needs BoxStream)
7. EventEmitter/EventSink (trait_variant::make)
8. VerbExecutor (async_trait - needs dyn dispatch array)
9. BuiltinTool (async_trait - sealed)
10. TaskScope (AFIT direct - generic splinters, not dyn)

### Key patterns

- **Bundle composition:** KernelBundle + IoBundle + LlmBundle (3 fields max per verb)
- **TaskScope splinters:** 6 sub-traits (TaskResults, BindingScope, MediaStaging, RecordStore, VaultLookup, InvocationContext). Each verb takes only what it needs via where clause.
- **linkme distributed_slice** for builtin/transform/lint/exporter/provider registration
- **thiserror 2 `#[error(transparent)] + #[from]`** for error domain promotion
- **EventEmitter blanket impl** on `Arc<T>` -> zero big-bang for 428 call sites
- **Redaction spine mandatory** constructor-injected, impossible to bypass
- **Conformance tests** per trait: same test on prod impl AND mock

---

## CRITICAL BUGS TO FIX FIRST (before refactor)

Found by 12 research agents, confirmed by code audit:

1. **`infer.rs:1072` dead receiver** — `mpsc::channel(1)` with `_rx` dropped. Stream chunks allocated then dropped. BOTH correctness and perf bug. Fix: wire real receiver OR check `tx.is_closed()`.

2. **IndexedDag NEVER USED** — `IndexedDag` exists with O(1) lookups but Runner uses hashmap-keyed `Dag`. The optimization was built and abandoned. Fix: switch `Runner::flow_graph` to `IndexedDag`.

3. **`nika-serve/worker.rs:124,170,192` tokio::sync::Mutex across .await** — deadlock latent. Fix: replace with DashMap or restructure.

4. **`util/interner.rs:15` intern() doesn't intern** — just `Arc::from(s)`, zero dedup. Fix: real interner with `FxHashMap<u64, Weak<str>>`.

5. **3 redaction gaps in trace files** — `TraceWriter::append_event` serializes UNREDACTED events. API keys, prompts, secrets written verbatim to .nika/traces/*.ndjson. Fix: mandatory redaction gate.

---

## 20 PHASES — EXECUTION ORDER

```
Pre-0: Write ARCHITECTURE.md in tools/nika-engine/              [matklad rule]
Phase 0: Shield Sprint 2 MERGED                                  [DONE - v0.79.0]
Phase 1: Kernel traits land in nika-core (or nika-kernel L0.5)  [J-26 to J-25]
Phase 2: nika-macros crate (4 derives + transform!)              [J-25 to J-23]
Phase 3: Wire EventEmitter (blanket impl, 2 commits)            [J-23 to J-22]
Phase 4: Promote error_domains (1 big-bang commit)               [J-22 to J-20]
Phase 5: Adopt rstest on pilot file                              [J-20 to J-19]
Phase 6: God file mechanical splits (parallel)                   [J-19 to J-17]
Phase 7: Consolidate LSP (nika-engine/lsp/ -> nika-lsp-core)    [J-17 to J-16]
Phase 8: Extract nika-provider                                   [J-16 to J-14]
Phase 9: Extract nika-runtime                                    [J-14 to J-11]
Phase 10: Extract nika-http                                      [J-11 to J-10]
Phase 11: Extract nika-exec-runner                               [J-10 to J-9]
Phase 12: Extract nika-fs                                        [J-9 to J-8]
Phase 13: Extract nika-builtin                                   [J-8 to J-6]
Phase 14: Extract nika-cache                                     [J-6 to J-5]
Phase 15: Migrate main.rs -> nika-cli/verbs/                     [J-5 to J-4]
Phase 16: analyze.rs split (11 files)                            [J-4 to J-3]
Phase 17: Split nika-tui + wire dead features                    [J-3 to J-2]
Phase 18: Type system hardening                                  [J-2 to J-1]
Phase 19: Polish + validation                                    [J-1 to J-0]
```

**Parallel opportunities:** P1+P2 | P3+P4+P5 | P6 sub-files | P10+P11+P12

### Per-phase prompt template

```
Execute Phase X of Nika Constellation v2.1 Architecture Refactor.

Read first:
- /Users/thibaut/dev/supernovae/nika/docs/plans/2026-04-08-constellation-v2-mega-plan.md
- /Users/thibaut/dev/supernovae/nika/CLAUDE.md
- /Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md

Philosophy: CONNECT, DO NOT DELETE. Big-bang OK (v0).

Constraints:
- cargo test --workspace --lib after every commit (10666+ tests)
- cargo clippy --workspace -- -D warnings (zero warnings)
- 1 fix = 1 commit. Format: refactor(arch): <what>
- Co-author: Nika 🦋 <nika@supernovae.studio>
- If code looks "unused", CONNECT IT, don't delete

Goal: <specific goal>
Files to CREATE: <list>
Files to MODIFY: <list>

Verification: cargo test + clippy + phase-specific metric
```

---

## KEY NUMBERS TO TRACK

| Metric | Current (v0.79.0) | Target (v0.80.0) |
|--------|-------------------|-------------------|
| Crates | 17 | ~44 |
| LOC nika-engine | 160k | 0 (split into runtime+provider+http+exec+builtin) |
| Largest file | runner/ 7100 | <1500 LOC |
| Binary size | 112 MB | 45 MB (minimal preset) |
| Tests | 10,666+ | 12,000+ |
| `.unwrap()` | 9,269 | <5,000 |
| Traits (pub) | 17 | 27+ |
| EventKind variants | 100 | 77 (13 sub-enums) |
| NikaError variants | 114 | 10 domain enums |
| Feature flags (dead) | 17 | 0 |
| cfg sites | 612 | <400 (LSP extracted) |
| Proc-macros | 0 | 4 derives + 1 declarative |
| Benchmarks | 2 | 9+ |
| Conformance tests | 0 | ~110 |
| Architectural invariants enforced | 0 | 62 (via xtask) |

---

## IMMEDIATE QUICK WINS (ship before Phase 1)

These can be done NOW, 1 commit each, zero risk:

1. **Fix `infer.rs:1072` dead receiver** — correctness bug
2. **Switch Runner to IndexedDag** — performance win, zero risk
3. **Add `#[must_use]` to all Runner builder methods** — prevents silent bugs
4. **Replace 3 `std::collections::HashSet` with `FxHashSet`** in template.rs
5. **`std::mem::take` instead of `.clone()` for StreamChunk::Done** at provider_streaming.rs:134
6. **Demote RunContext `workspace_root: Arc<RwLock<PathBuf>>` to `OnceLock`** — write-once field
7. **Hoist `Arc::from(task_id)` out of streaming loop** at streaming.rs:165

---

## RESEARCH COMPLETED (12 agents, all findings integrated)

### Wave 2 (ecosystem mapping)
1. Complete ecosystem graph — 100 EventKind, 114 NikaError, 63 builtins, 612 cfg sites, health score 78/100
2. 12 user journeys traced — kernel functions identified, dead code candidates
3. Extension points blueprint — 14 categories, nika-plugin crate, 7 linkme slices, 4 delivery modes
4. Feature matrix + config hierarchy — 10 anti-patterns, nika-config crate, 8-layer precedence
5. Observability spine — nika-telemetry, 13 sub-enums, 37 metrics, span hierarchy, redaction mandatory
6. Testing strategy — 62 invariants, xtask enforcement, nika-conformance crate, CI 19 jobs

### Wave 3 (deep patterns)
7. Context7 Rust 2024 — AFIT, trait_variant, linkme, thiserror 2, syn 2, sealed, schemars, tokio patterns
8. Ecosystem case studies — Bevy 58, rust-analyzer 36, Helix 13, Zed 231, Tokio, Ruff, DDD patterns
9. Idiomatic hot paths — 6 hot paths graded C-D, 10 allocation hotspots, 10 invariants
10. Conceptual integrity DDD — 10 bounded contexts, 20 integrity rules, 10 drift signs, ubiquitous language
11. 5 verb crate architecture — per-verb state structs, 7 cross-verb shared patterns, nika-verb-util
12. Performance audit — 112 MB binary, top 10 hot paths, 15 invariants, 7 benchmarks, binary size plan

---

## GIT STATE

```
Branch: main
Last commit: 70f0022dc chore: bump version to v0.79.0 in npm packages, vscode extension, docs
Tag: v0.79.0 (Shield)
Clean working tree (after Shield merge)
Velocity April 2026: ~102 commits/day
```

---

## AUTONOMY LEVEL

**Full autonomous execution authorized** for:
- Refactoring (any file, any crate)
- Creating new crates
- Moving code between crates
- Renaming types/traits/modules
- Promoting dead scaffolding
- Splitting god files
- Adding traits and impls
- Adding tests and benchmarks

**Requires explicit approval** for:
- Changing the 5 verbs (NEVER)
- Changing the workflow schema version (@0.12)
- Adding new external dependencies (except linkme, trait-variant, schemars)
- Deleting features (connect, don't delete)
- Changing the AGPL license

**Rules always apply:**
- `cargo test --workspace --lib` after EVERY commit
- `cargo clippy --workspace -- -D warnings` — zero warnings
- Co-author: `Nika 🦋 <nika@supernovae.studio>` (NEVER Claude/Anthropic)
- 1 logical change = 1 commit
- Format: `refactor(arch): <what>` or `feat(X): <what>`
- Tests move WITH their code
- Run `editors/sync-editors.sh --fix` if catalogs change

---

## LAUNCH TARGET

**May 5, 2026 — Show HN: "Inference as Code"**

Show HN messaging rules (from research):
- Lead with the primitive: "One YAML file. Any AI provider."
- Lead with user-facing numbers: 9 providers, 5 verbs, 65 transforms, 63 builtins
- NEVER mention the crate count (it's proof, not claim)
- Single binary: `brew install nika`
- Show the YAML in the first 200 words
- Architecture depth in "Technical details" section, not the lede
- Benchmark chart vs LangChain/Airflow/Dagster

Marketing kit ready at `docs/marketing/` (17 files: HN, Product Hunt, Dev.to, Twitter, press, one-pager, video script, podcast, email, competitor matrix).

---

## START HERE

1. Read `docs/plans/2026-04-08-constellation-v2-mega-plan.md` (focus sections 3, 5, 8, 17, 18)
2. Write `tools/nika-engine/ARCHITECTURE.md` (Pre-Phase 0, matklad rule)
3. Fix the 5 critical bugs listed above
4. Begin Phase 1: create nika-kernel with 10 trait definitions

**Good luck. Make Nika the most elegant Rust workflow engine in existence.**
